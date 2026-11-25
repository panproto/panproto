//! Table lens put over a view held as the segments of a partition.
//!
//! [`put_segmented`] restores a table view laid out as segments (the output
//! of a segmented table get, possibly edited) against the complement
//! segments of that get, as a checkpointed run of [`TablePut`], one task per
//! segment and phase, never assembling the view or the source. Each segment
//! restores the rows it owns and the pairs whose source row it owns against
//! its own complement segment, by the restoration whole-state
//! [`put`](super::put) runs, with the rows other segments own held as
//! placeholders. What a segment reads of another segment's rows arrives in
//! two steps:
//!
//! - one exchange of row facts: each segment asks the owner of every remote
//!   row its pairs, dropped pairs and synthesized pairs name whether its
//!   view holds the row, with which view table, and which source table the
//!   row restores to;
//! - after restoration, the boundary of the restored family: every restored
//!   pair entering another segment becomes an in-stub there, and every
//!   remote target a ghost.
//!
//! The clauses read on the restored source run over the restored family
//! segment by segment: the get of the restored source before and after the
//! view's cell edits are pulled back is the segmented table get of
//! [`table_restrict_segmented_with`], whose contraction crosses segments
//! through its reach summaries and whose refusal is recomputed in the
//! segment owning its element. Every segment's violations are collected and
//! the least in report order is returned, so a refused put changes no
//! segment.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_inst::identity::{CanonicalSink, ComplementShape, Digest, SEGMENT_COMPLEMENT_CONTEXT};
use panproto_inst::locality::{GlobalReason, Locality, SegmentOp, SummaryKind};
use panproto_inst::segment::{
    AnySegment, ArtifactKind, Boundary, Budget, Emission, Ghost, InStub, ManifestRole, MemoryModel,
    OwnerIndex, Phase, PhaseContext, PhaseOperation, PhaseStop, Reader, RunSpec, SegmentError,
    SegmentId, SegmentRecord, SegmentSource, SegmentStore, SegmentedRestrictError, SegmentedRun,
    StreamStop, TableComplementSegment, TableSegment, TaskKey, graph_restrict_locality,
    read_boundary, run_segmented, store_failure, table_restrict_segmented_with, write_boundary,
};
use panproto_inst::{
    BoundaryProtocols, CellPlan, DroppedRow, EntityFunctor, FInstance, FieldTransform,
    PutDomainError, RestrictPlan, Row, RowId, TableComplement, ViewRef,
    functor_restrict_finite_with_plan,
};
use panproto_schema::{CanonicalWriter, Edge, InstanceShapeKind, KeyRole, Protocol, Schema};

use super::{
    compile, id_collisions, pull_changed_rows, refusal_from_restrict, restore_dropped_pairs,
    restore_rows, restore_view_pairs, schema_violations, stability_findings, synthesized_edits,
    table_transform_traces,
};
use crate::graph_lens::{HeldView, SegmentedPutError};
use crate::{Lens, LensError};

/// The operation bytes of table put in a run's identity.
pub const TABLE_PUT_OP: &[u8] = b"table put";

/// The locality class of table put along `lens` at `protocols`.
///
/// Put restricts the restored source again to check forward stability, so
/// a restrict that [`graph_restrict_locality`] classifies as global is
/// global here too. A uniqueness or primary key of the source schema is
/// [`GlobalReason::KeyDemand`] at the least such table and key. Otherwise put
/// exchanges row facts across the boundary and its forward-stability get
/// follows reach summaries, so its class is [`Locality::Exchange`] with the
/// restrict's class.
///
/// # Errors
///
/// The table plan's compilation error.
pub fn table_put_locality(
    lens: &Lens,
    protocols: BoundaryProtocols<'_>,
) -> Result<Locality, LensError> {
    Ok(put_locality(lens, &compile(lens, protocols)?))
}

fn put_locality(lens: &Lens, plan: &RestrictPlan) -> Locality {
    let restrict = graph_restrict_locality(plan);
    if matches!(restrict, Locality::Global(_)) {
        return restrict;
    }
    if let Some(reason) = key_demand(&lens.src_schema) {
        return Locality::Global(reason);
    }
    Locality::of_clauses([
        restrict,
        Locality::Exchange,
        Locality::Summary(SummaryKind::Reach),
    ])
}

/// The least uniqueness or primary key of the source schema: its values
/// are compared across every row of its table.
fn key_demand(schema: &Schema) -> Option<GlobalReason> {
    schema
        .declarations
        .keys
        .iter()
        .flat_map(|(table, keys)| keys.iter().map(move |key| (table, key)))
        .filter(|(_, key)| {
            matches!(
                key.role,
                KeyRole::Primary | KeyRole::Unique | KeyRole::Identifier
            )
        })
        .map(|(table, key)| (table.clone(), key.id.clone()))
        .min()
        .map(|(vertex, key)| GlobalReason::KeyDemand { vertex, key })
}

/// What an owner tells a segment of one of its rows: the view table when
/// its view holds the row, and the source table it restores to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RowFact {
    shown: Option<Name>,
    restored: Option<Name>,
}

/// One segment's view segment and complement segment.
struct Part {
    view: TableSegment,
    complement: TableComplementSegment,
}

impl Part {
    /// The remote rows this part's restoration reads: the targets of its
    /// view pairs, its dropped pairs and its synthesized pairs that another
    /// segment owns.
    fn requests(&self, index: &OwnerIndex) -> BTreeSet<RowId> {
        let mut rows: BTreeSet<RowId> = self.view.pairs.iter().map(|(_, _, tgt)| *tgt).collect();
        rows.extend(
            self.complement
                .dropped_fk_pairs
                .values()
                .flat_map(|pairs| pairs.iter().map(|(_, tgt)| *tgt)),
        );
        rows.extend(
            self.complement
                .synthesized_fk_pairs
                .iter()
                .map(|(_, _, tgt)| *tgt),
        );
        rows.retain(|row| index.owner_of_node(row.0) != Some(self.view.segment));
        rows
    }

    /// The facts this part's owner offers of the rows `wanted`, from its
    /// own view and complement segments.
    fn offer(
        &self,
        lens: &Lens,
        (functor, cells): (&EntityFunctor, &CellPlan),
        wanted: &BTreeSet<RowId>,
    ) -> Result<BTreeMap<RowId, RowFact>, SegmentError> {
        let view = owned_view(&self.view)?;
        let complement = TableComplementSegment::glue(std::slice::from_ref(&self.complement));
        let (restored, _) = restore_rows(lens, &view, &complement, functor, cells);
        Ok(wanted
            .iter()
            .map(|row| {
                let fact = RowFact {
                    shown: self.view.rows.get(row).map(|(table, _)| table.clone()),
                    restored: restored.table_of(*row).cloned(),
                };
                (*row, fact)
            })
            .collect())
    }
}

/// The view segment's rows and pairs, with a row without cells for each
/// ghost, as one table.
fn owned_view(view: &TableSegment) -> Result<FInstance, SegmentError> {
    let placeholders = view
        .boundary
        .ghosts
        .values()
        .filter(|ghost| !view.rows.contains_key(&RowId(ghost.node)))
        .map(|ghost| (RowId(ghost.node), ghost.anchor.clone()));
    table_of_parts(&view.rows, placeholders, &view.pairs)
}

/// The table holding `rows`, a row without cells of each table of
/// `placeholders`, and `pairs`.
fn table_of_parts(
    rows: &BTreeMap<RowId, (Name, Row)>,
    placeholders: impl IntoIterator<Item = (RowId, Name)>,
    pairs: &BTreeSet<(Edge, RowId, RowId)>,
) -> Result<FInstance, SegmentError> {
    let mut by_table: BTreeMap<Name, Vec<(RowId, Row)>> = BTreeMap::new();
    for (id, (table, row)) in rows {
        by_table
            .entry(table.clone())
            .or_default()
            .push((*id, row.clone()));
    }
    for (id, table) in placeholders {
        by_table.entry(table).or_default().push((id, Row::new()));
    }
    let mut by_edge: BTreeMap<Edge, Vec<(RowId, RowId)>> = BTreeMap::new();
    for (edge, src, tgt) in pairs {
        by_edge.entry(edge.clone()).or_default().push((*src, *tgt));
    }
    FInstance::from_parts(by_table, by_edge).map_err(|error| SegmentError::Decode {
        artifact: ArtifactKind::Segment,
        message: format!("a table segment does not form a table: {error}"),
    })
}

/// One segment's restoration: its view and complement with a placeholder
/// for every remote row it reads, the restored source of its rows and
/// those placeholders, and the violations found at its own elements.
struct Restoration {
    segment: SegmentId,
    view: FInstance,
    complement: TableComplement,
    source: FInstance,
    /// Id collisions and edited synthesized pairs.
    pre: Vec<PutDomainError>,
    /// The violations of restoring rows and pairs.
    errors: Vec<PutDomainError>,
}

/// One segment's source after the view's cell edits are pulled back.
struct Pulled {
    source: FInstance,
    /// The traces of the rows it owns that the baseline view holds.
    traces: BTreeMap<RowId, Vec<FieldTransform>>,
    /// The refusal of tracing the least such row, when it refuses.
    trace_refusal: Option<PutDomainError>,
    /// The violations of pulling the edits back.
    errors: Vec<PutDomainError>,
}

/// The row a put-domain violation is about: a row's own, a pair's source
/// row; `None` for a violation about no single element.
const fn about(error: &PutDomainError) -> Option<RowId> {
    match error {
        PutDomainError::IdCollision { elem }
        | PutDomainError::SynthesizedEdited { elem }
        | PutDomainError::ContractedEdgeInserted { elem }
        | PutDomainError::UnrepresentableViewEdge { elem }
        | PutDomainError::AmbiguousReverseAnchor { elem, .. }
        | PutDomainError::SurvivalPredicateFails { elem }
        | PutDomainError::DerivedCellEdited { elem, .. }
        | PutDomainError::TransformNotInvertible { elem, .. }
        | PutDomainError::MissingRequiredCell { elem, .. }
        | PutDomainError::SurvivalPredicateRefused { elem, .. } => match elem {
            ViewRef::Row(row) | ViewRef::Pair { src: row, .. } => Some(*row),
            _ => None,
        },
        _ => None,
    }
}

/// The table put a run executes: the lens, its plan's table functor and
/// cell plan, and the source protocol the restored source is validated
/// under.
struct Engine<'a> {
    lens: &'a Lens,
    plan: &'a RestrictPlan,
    functor: &'a EntityFunctor,
    cells: &'a CellPlan,
    protocol: &'a Protocol,
}

impl Engine<'_> {
    /// Restore `part` against its complement, with the facts `facts` of the
    /// remote rows it reads and the family's floor `floor`.
    fn restore(
        &self,
        part: &Part,
        facts: &BTreeMap<RowId, RowFact>,
        floor: u32,
        index: &OwnerIndex,
    ) -> Result<Restoration, SegmentError> {
        let segment = part.view.segment;
        let mine = |row: RowId| index.owner_of_node(row.0) == Some(segment);
        let mut placeholders: BTreeMap<RowId, Name> = part
            .view
            .boundary
            .ghosts
            .values()
            .filter(|ghost| !mine(RowId(ghost.node)))
            .map(|ghost| (RowId(ghost.node), ghost.anchor.clone()))
            .collect();
        for (row, fact) in facts {
            if let (false, Some(shown)) = (mine(*row), &fact.shown) {
                placeholders.entry(*row).or_insert_with(|| shown.clone());
            }
        }
        let view = table_of_parts(&part.view.rows, placeholders, &part.view.pairs)?;
        let mut complement = TableComplementSegment::glue(std::slice::from_ref(&part.complement));
        complement.id_floor = floor;
        for (row, fact) in facts.iter().filter(|(row, _)| !mine(**row)) {
            match (&fact.shown, &fact.restored) {
                (Some(_), Some(table)) => {
                    complement.original_tables.insert(*row, table.clone());
                }
                (None, Some(table)) => {
                    complement.dropped_rows.insert(
                        *row,
                        DroppedRow {
                            table: table.clone(),
                            row: Row::new(),
                        },
                    );
                }
                _ => {}
            }
        }
        let (mut source, mut errors) =
            restore_rows(self.lens, &view, &complement, self.functor, self.cells);
        restore_view_pairs(
            &view,
            &complement,
            self.plan,
            self.functor,
            &mut source,
            &mut errors,
        );
        restore_dropped_pairs(&complement, &mut source);
        let mut pre = id_collisions(&view, &complement);
        pre.extend(synthesized_edits(&view, &complement));
        let owned = |error: &PutDomainError| about(error).is_some_and(mine);
        pre.retain(owned);
        errors.retain(owned);
        Ok(Restoration {
            segment,
            view,
            complement,
            source,
            pre,
            errors,
        })
    }

    /// Pull the view's cell edits of `restoration` back into its source,
    /// against the baseline view: the get of its restored source, of which
    /// the rows it owns are those of the family's get.
    fn pull(&self, restoration: &Restoration, index: &OwnerIndex) -> Result<Pulled, SegmentError> {
        let mine = |row: RowId| index.owner_of_node(row.0) == Some(restoration.segment);
        let (local, _) = functor_restrict_finite_with_plan(&restoration.source, self.plan)
            .map_err(|error| SegmentError::Decode {
                artifact: ArtifactKind::Assembly,
                message: format!("a restored segment's get refuses: {error}"),
            })?;
        let baseline = owned_rows(&local, &mine)?;
        let mut source = restoration.source.clone();
        let traces = match table_transform_traces(self.lens, &restoration.source, &baseline) {
            Ok(traces) => traces,
            Err(error) => {
                return Ok(Pulled {
                    source,
                    traces: BTreeMap::new(),
                    trace_refusal: refusal_from_restrict(error).into_iter().next(),
                    errors: Vec::new(),
                });
            }
        };
        let mut errors = pull_changed_rows(
            &restoration.view,
            &baseline,
            self.functor,
            self.cells,
            &traces,
            &mut source,
        );
        errors.retain(|error| about(error).is_some_and(mine));
        Ok(Pulled {
            source,
            traces,
            trace_refusal: None,
            errors,
        })
    }

    /// The forward-stability violations of one segment, given its view
    /// segment `regenerated` of the family's get of the restored source:
    /// the trace refusal of the least row whose regenerated trace refuses,
    /// or every violation at the segment's own elements.
    fn stability(
        &self,
        restoration: &Restoration,
        pulled: &Pulled,
        regenerated: &TableSegment,
        index: &OwnerIndex,
    ) -> Result<Result<Vec<PutDomainError>, PutDomainError>, SegmentError> {
        let mine = |row: RowId| index.owner_of_node(row.0) == Some(restoration.segment);
        let rows = table_of_parts(&regenerated.rows, [], &BTreeSet::new())?;
        let mut traces = pulled.traces.clone();
        match table_transform_traces(self.lens, &pulled.source, &rows) {
            Ok(replayed) => {
                for (id, trace) in replayed {
                    traces.entry(id).or_default().extend(trace);
                }
            }
            Err(error) => {
                return refusal_from_restrict(error)
                    .into_iter()
                    .next()
                    .map_or_else(|| Ok(Ok(Vec::new())), |refusal| Ok(Err(refusal)));
            }
        }
        let view = owned_view(regenerated)?;
        let mut found = stability_findings(
            &restoration.view,
            &restoration.complement,
            &view,
            &pulled.source,
            self.plan,
            &traces,
        );
        found.retain(|error| about(error).is_some_and(mine));
        Ok(Ok(found))
    }

    /// The required cells missing at the rows a segment owns in its pulled
    /// source.
    fn required(
        &self,
        pulled: &Pulled,
        segment: SegmentId,
        index: &OwnerIndex,
    ) -> Vec<PutDomainError> {
        let mut found = schema_violations(&pulled.source, &self.lens.src_schema, self.protocol);
        found.retain(|error| {
            matches!(error, PutDomainError::MissingRequiredCell { .. })
                && about(error).is_some_and(|row| index.owner_of_node(row.0) == Some(segment))
        });
        found
    }
}

/// The rows of `table` that `mine` admits, without pairs.
fn owned_rows(table: &FInstance, mine: &dyn Fn(RowId) -> bool) -> Result<FInstance, SegmentError> {
    let rows: BTreeMap<RowId, (Name, Row)> = table
        .row_ids()
        .into_iter()
        .filter(|id| mine(*id))
        .filter_map(|id| Some((id, (table.table_of(id)?.clone(), table.row(id)?.clone()))))
        .collect();
    table_of_parts(&rows, [], &BTreeSet::new())
}

/// A crossing of one restored pair: the owner of its target, the pair's
/// view reference and its in-stub there.
type Crossing = (SegmentId, ViewRef, InStub);

/// The segment `segment` of the restored family from its restored source:
/// the rows it owns and its pairs, with a ghost for each target owned
/// elsewhere, and the crossings of those pairs.
fn family_segment(
    segment: SegmentId,
    source: &FInstance,
    index: &OwnerIndex,
) -> Result<(TableSegment, Vec<Crossing>), SegmentError> {
    let mine = |row: RowId| index.owner_of_node(row.0) == Some(segment);
    let mut out = TableSegment {
        segment,
        rows: BTreeMap::new(),
        pairs: BTreeSet::new(),
        boundary: Boundary::default(),
    };
    for id in source.row_ids().into_iter().filter(|id| mine(*id)) {
        if let (Some(table), Some(row)) = (source.table_of(id), source.row(id)) {
            out.rows.insert(id, (table.clone(), row.clone()));
        }
    }
    let mut crossings = Vec::new();
    for (edge, src, tgt) in source.all_pairs() {
        if !mine(src) {
            continue;
        }
        out.pairs.insert((edge.clone(), src, tgt));
        if mine(tgt) {
            continue;
        }
        let elem = ViewRef::Pair {
            edge: edge.clone(),
            src,
            tgt,
        };
        let owner = index
            .owner_of_node(tgt.0)
            .ok_or_else(|| SegmentError::Unowned { elem: elem.clone() })?;
        let table = |row: RowId| source.table_of(row).cloned().unwrap_or_default();
        out.boundary.ghosts.entry(tgt.0).or_insert_with(|| Ghost {
            node: tgt.0,
            owner,
            anchor: table(tgt),
        });
        crossings.push((
            owner,
            elem,
            InStub {
                src: src.0,
                src_owner: segment,
                src_anchor: table(src),
                tgt: tgt.0,
                label: edge,
            },
        ));
    }
    Ok((out, crossings))
}

/// Check that every entry of `complement` is about a row its segment owns:
/// a row's entries, and a pair's by the owner of its source row.
fn check_complement(
    complement: &TableComplementSegment,
    index: &OwnerIndex,
) -> Result<(), SegmentError> {
    let holder = complement.segment;
    let place = |elem: ViewRef, row: RowId| match index.owner_of_node(row.0) {
        None => Err(SegmentError::Unowned { elem }),
        Some(owner) if owner != holder => Err(SegmentError::WrongOwner {
            elem,
            expected: owner,
            found: holder,
        }),
        Some(_) => Ok(()),
    };
    let rows = complement
        .dropped_rows
        .keys()
        .chain(complement.dropped_cells.keys().map(|(row, _)| row))
        .chain(complement.original_tables.keys())
        .chain(complement.original_cells.keys().map(|(row, _)| row))
        .chain(complement.original_absent_cells.iter().map(|(row, _)| row));
    for row in rows {
        place(ViewRef::Row(*row), *row)?;
    }
    let pairs = complement
        .dropped_fk_pairs
        .iter()
        .flat_map(|(edge, pairs)| pairs.iter().map(move |(src, tgt)| (edge, *src, *tgt)))
        .chain(
            complement
                .original_fk_edges
                .keys()
                .chain(&complement.synthesized_fk_pairs)
                .map(|(edge, src, tgt)| (edge, *src, *tgt)),
        );
    for (edge, src, tgt) in pairs {
        place(
            ViewRef::Pair {
                edge: edge.clone(),
                src,
                tgt,
            },
            src,
        )?;
    }
    Ok(())
}

/// Table put along a lens as a [`PhaseOperation`] of the checkpointed
/// runner, over the view segments the input manifest seals and the
/// complement segments `complements` holds.
///
/// Its phases, each one task per segment except the coordinator's
/// Assemble:
///
/// - Validate reads a view segment (checked against the view manifest by
///   the runner) and its complement segment's canonical bytes
///   ([`TableComplementSegment::from_canonical_bytes`]), checks that every
///   complement entry is about a row the segment owns, and writes the
///   eight-byte length of the view bytes, the view bytes and the complement
///   bytes. The complement's digest is among the task's inputs, and the
///   working set of a segment is its view's estimate plus its complement's
///   length.
/// - Export writes the segment's complement floor and its row-fact
///   requests by owner.
/// - Summarize answers every request addressed to the segment with the
///   row's fact (its view table and the source table it restores to),
///   restored from the segment's own view and complement.
/// - Assemble restores every segment from its part, the family's floor and
///   its owners' facts, one segment at a time, and lays out the restored
///   family's boundary; then runs the clauses read on the restored source
///   in whole-state put's order: restoration, the get of the restored
///   source ([`table_restrict_segmented_with`], fetching each segment by
///   restoring it again), pulling the view's cell edits back, forward
///   stability against the get of the pulled source, and required cells.
///   The least violation is the run's refusal, so a refused put emits and
///   seals nothing; otherwise its artifact is the boundary of every
///   restored segment.
/// - Emit restores the segment again, pulls its cell edits back, and
///   writes it with its boundary from the assembly; it emits no
///   complement.
/// - Seal seals the restored segments as a source.
///
/// The coordinator holds the floors, the row facts and the boundaries,
/// which grow with the boundary, and the reach summaries and realized pairs
/// of the restored source's get; never two segments.
pub struct TablePut<'a, C> {
    engine: Engine<'a>,
    complements: &'a C,
}

type PutStop = PhaseStop<SegmentedPutError>;

fn position(segment: SegmentId) -> usize {
    usize::try_from(segment.0).unwrap_or(usize::MAX)
}

fn refused(error: PutDomainError) -> PutStop {
    PhaseStop::Failed(SegmentedPutError::Lens(LensError::PutDomain(Box::new(
        error,
    ))))
}

/// The least violation in report order.
fn least(mut found: Vec<PutDomainError>) -> Option<PutDomainError> {
    PutDomainError::sort_report(&mut found);
    found.into_iter().next()
}

/// A refusal of the restored source's get as a put stop: the get's
/// refusal as the clause's violation, any other stop as it is.
enum GetStop {
    Refused(Box<PutDomainError>),
    Stop(PutStop),
}

impl From<StreamStop<PutStop>> for GetStop {
    fn from(stop: StreamStop<PutStop>) -> Self {
        match stop {
            StreamStop::Source(stop) => Self::Stop(stop),
            StreamStop::Refused(SegmentedRestrictError::Restrict { error, .. }) => {
                refusal_from_restrict(error).into_iter().next().map_or_else(
                    || {
                        Self::Stop(PhaseStop::Failed(SegmentedPutError::Lens(
                            LensError::ComplementMismatch {
                                detail: "the restored source's get refuses".to_owned(),
                            },
                        )))
                    },
                    |error| Self::Refused(Box::new(error)),
                )
            }
            StreamStop::Refused(SegmentedRestrictError::Segment(error)) => {
                Self::Stop(PhaseStop::from(error))
            }
            StreamStop::Refused(SegmentedRestrictError::Residual(residual)) => {
                Self::Stop(PhaseStop::Residual(residual))
            }
            StreamStop::Refused(other) => Self::Stop(PhaseStop::Failed(SegmentedPutError::Lens(
                LensError::ComplementMismatch {
                    detail: other.to_string(),
                },
            ))),
        }
    }
}

/// What every segment's restoration reads, as the coordinator holds it:
/// the family's floor and the facts of every requested row.
struct Shared {
    floor: u32,
    facts: BTreeMap<RowId, RowFact>,
}

impl<C: SegmentSource> TablePut<'_, C> {
    fn validated(cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Part, PutStop> {
        let bytes = cx.artifact(Phase::Validate, Some(segment))?;
        let mut r = Reader::new(&bytes, ArtifactKind::Segment);
        let len = usize::try_from(r.u64()?).unwrap_or(usize::MAX);
        let view = r.take(len)?;
        let complement = r.take(bytes.len().saturating_sub(8).saturating_sub(len))?;
        r.finish()?;
        let AnySegment::Table(view) = AnySegment::from_canonical_bytes(view, cx.partition())?.0
        else {
            return Err(PutStop::from(SegmentError::Decode {
                artifact: ArtifactKind::Segment,
                message: "not a table segment".to_owned(),
            }));
        };
        let complement = TableComplementSegment::from_canonical_bytes(complement, segment)?;
        Ok(Part { view, complement })
    }

    /// Every segment's Export, in segment order.
    fn exports(cx: &PhaseContext<'_>) -> Result<Vec<(SegmentId, Exported)>, PutStop> {
        cx.segments()
            .into_iter()
            .map(|segment| {
                let bytes = cx.artifact(Phase::Export, Some(segment))?;
                Ok((segment, Exported::decode(&bytes)?))
            })
            .collect()
    }

    /// The floor of the family and the facts the owners of `segments`
    /// offered.
    fn shared(
        cx: &PhaseContext<'_>,
        exports: &[(SegmentId, Exported)],
        owners: impl IntoIterator<Item = SegmentId>,
    ) -> Result<Shared, PutStop> {
        let floor = exports
            .iter()
            .map(|(_, exported)| exported.floor)
            .max()
            .unwrap_or(0);
        let mut facts = BTreeMap::new();
        for owner in owners {
            facts.extend(decode_facts(&cx.artifact(Phase::Summarize, Some(owner))?)?);
        }
        Ok(Shared { floor, facts })
    }

    /// Segment `segment`'s restoration, with the facts of the rows it
    /// requested.
    fn restoration(
        &self,
        cx: &PhaseContext<'_>,
        segment: SegmentId,
        shared: &Shared,
    ) -> Result<Restoration, PutStop> {
        let part = Self::validated(cx, segment)?;
        let facts: BTreeMap<RowId, RowFact> = part
            .requests(cx.index())
            .into_iter()
            .filter_map(|row| Some((row, shared.facts.get(&row)?.clone())))
            .collect();
        Ok(self
            .engine
            .restore(&part, &facts, shared.floor, cx.index())?)
    }

    /// Segment `segment` of the restored family before (`pulled` false)
    /// or after its cell edits are pulled back, with its boundary.
    fn family_member(
        &self,
        cx: &PhaseContext<'_>,
        segment: SegmentId,
        shared: &Shared,
        boundaries: &[Boundary],
        pulled: bool,
    ) -> Result<TableSegment, PutStop> {
        let restoration = self.restoration(cx, segment, shared)?;
        let source = if pulled {
            self.engine.pull(&restoration, cx.index())?.source
        } else {
            restoration.source
        };
        let (mut member, _) = family_segment(segment, &source, cx.index())?;
        member.boundary =
            boundaries
                .get(position(segment))
                .cloned()
                .ok_or_else(|| SegmentError::Decode {
                    artifact: ArtifactKind::Assembly,
                    message: "a boundary is missing".to_owned(),
                })?;
        Ok(member)
    }

    /// Run the restored family's get, before or after the pull, giving
    /// `visit` each segment's view segment.
    fn family_get(
        &self,
        cx: &PhaseContext<'_>,
        shared: &Shared,
        boundaries: &[Boundary],
        pulled: bool,
        visit: &mut dyn FnMut(SegmentId, TableSegment) -> Result<(), PutStop>,
    ) -> Result<(), GetStop> {
        let ids = cx.segments();
        table_restrict_segmented_with::<PutStop>(
            self.engine.plan,
            &ids,
            cx.index(),
            &mut |segment| {
                self.family_member(cx, segment, shared, boundaries, pulled)
                    .map(Cow::Owned)
            },
            &mut |segment, view, _| visit(segment, view),
        )
        .map_err(GetStop::from)
    }

    /// The clauses read after restoration: the violations found, the
    /// restored source's get first.
    fn source_clauses(
        &self,
        cx: &PhaseContext<'_>,
        shared: &Shared,
        boundaries: &[Boundary],
    ) -> Result<Vec<PutDomainError>, PutStop> {
        let ids = cx.segments();
        match self.family_get(cx, shared, boundaries, false, &mut |_, _| Ok(())) {
            Ok(()) => {}
            Err(GetStop::Refused(error)) => return Ok(vec![*error]),
            Err(GetStop::Stop(stop)) => return Err(stop),
        }
        let mut trace_refusal: Option<PutDomainError> = None;
        let mut pulled_errors = Vec::new();
        for &segment in &ids {
            let restoration = self.restoration(cx, segment, shared)?;
            let pulled = self.engine.pull(&restoration, cx.index())?;
            if let Some(refusal) = pulled.trace_refusal {
                trace_refusal = least(
                    [trace_refusal, Some(refusal)]
                        .into_iter()
                        .flatten()
                        .collect(),
                );
            }
            pulled_errors.extend(pulled.errors);
        }
        if let Some(refusal) = trace_refusal {
            return Ok(vec![refusal]);
        }
        if !pulled_errors.is_empty() {
            return Ok(pulled_errors);
        }
        let mut found = Vec::new();
        let mut stability = Vec::new();
        let mut regenerated_refusal: Option<PutDomainError> = None;
        let outcome = self.family_get(cx, shared, boundaries, true, &mut |segment, view| {
            let restoration = self.restoration(cx, segment, shared)?;
            let pulled = self.engine.pull(&restoration, cx.index())?;
            match self
                .engine
                .stability(&restoration, &pulled, &view, cx.index())?
            {
                Ok(findings) => stability.extend(findings),
                Err(refusal) => {
                    regenerated_refusal = least(
                        [regenerated_refusal.take(), Some(refusal)]
                            .into_iter()
                            .flatten()
                            .collect(),
                    );
                }
            }
            Ok(())
        });
        match outcome {
            Ok(()) => match regenerated_refusal {
                Some(refusal) => found.push(refusal),
                None => found.extend(stability),
            },
            Err(GetStop::Refused(error)) => found.push(*error),
            Err(GetStop::Stop(stop)) => return Err(stop),
        }
        for &segment in &ids {
            let restoration = self.restoration(cx, segment, shared)?;
            let pulled = self.engine.pull(&restoration, cx.index())?;
            found.extend(self.engine.required(&pulled, segment, cx.index()));
        }
        Ok(found)
    }
}

/// One segment's Export: its complement's floor and the remote rows it
/// reads, by owner.
#[derive(Debug, Default)]
struct Exported {
    floor: u32,
    requests: BTreeMap<SegmentId, BTreeSet<RowId>>,
}

impl Exported {
    fn of(part: &Part, index: &OwnerIndex) -> Self {
        let mut requests: BTreeMap<SegmentId, BTreeSet<RowId>> = BTreeMap::new();
        for row in part.requests(index) {
            if let Some(owner) = index.owner_of_node(row.0) {
                requests.entry(owner).or_default().insert(row);
            }
        }
        Self {
            floor: part.complement.floor,
            requests,
        }
    }

    /// The canonical bytes: the floor, then each owner ascending with its
    /// requested rows ascending.
    fn bytes(&self) -> Vec<u8> {
        let mut sink = CanonicalWriter::new();
        sink.u32(self.floor);
        sink.len(self.requests.len());
        for (owner, rows) in &self.requests {
            sink.u32(owner.0);
            sink.len(rows.len());
            for row in rows {
                sink.u32(row.0);
            }
        }
        sink.into_bytes()
    }

    fn decode(bytes: &[u8]) -> Result<Self, SegmentError> {
        let mut r = Reader::new(bytes, ArtifactKind::Export);
        let floor = r.u32()?;
        let mut requests = BTreeMap::new();
        for _ in 0..r.count(8)? {
            let owner = SegmentId(r.u32()?);
            if requests
                .keys()
                .next_back()
                .is_some_and(|last| *last >= owner)
            {
                return Err(r.error("owners are not strictly ascending"));
            }
            let mut rows = BTreeSet::new();
            for _ in 0..r.count(4)? {
                let row = RowId(r.u32()?);
                if rows.last().is_some_and(|last| *last >= row) {
                    return Err(r.error("rows are not strictly ascending"));
                }
                rows.insert(row);
            }
            requests.insert(owner, rows);
        }
        r.finish()?;
        Ok(Self { floor, requests })
    }
}

/// The canonical bytes of an owner's facts: their count, then per row
/// ascending its id, its view table and its restored table, each a presence
/// byte and a name.
fn facts_bytes(facts: &BTreeMap<RowId, RowFact>) -> Vec<u8> {
    let mut sink = CanonicalWriter::new();
    sink.len(facts.len());
    for (row, fact) in facts {
        sink.u32(row.0);
        sink.opt_name(fact.shown.as_ref());
        sink.opt_name(fact.restored.as_ref());
    }
    sink.into_bytes()
}

fn decode_facts(bytes: &[u8]) -> Result<BTreeMap<RowId, RowFact>, SegmentError> {
    let mut r = Reader::new(bytes, ArtifactKind::Summary);
    let mut facts = BTreeMap::new();
    for _ in 0..r.count(6)? {
        let row = RowId(r.u32()?);
        if facts.keys().next_back().is_some_and(|last| *last >= row) {
            return Err(r.error("facts are not strictly ascending"));
        }
        let shown = r.opt_name()?;
        let restored = r.opt_name()?;
        facts.insert(row, RowFact { shown, restored });
    }
    r.finish()?;
    Ok(facts)
}

/// The Assemble artifact of a put: the number of segments and each
/// restored segment's boundary, in segment order.
fn boundaries_bytes(boundaries: &[Boundary]) -> Vec<u8> {
    let mut sink = CanonicalWriter::new();
    sink.len(boundaries.len());
    for boundary in boundaries {
        write_boundary(&mut sink, boundary);
    }
    sink.into_bytes()
}

fn decode_boundaries(bytes: &[u8]) -> Result<Vec<Boundary>, SegmentError> {
    let mut r = Reader::new(bytes, ArtifactKind::Assembly);
    let n = r.count(16)?;
    let mut boundaries = Vec::with_capacity(n);
    for _ in 0..n {
        boundaries.push(read_boundary(&mut r)?);
    }
    r.finish()?;
    Ok(boundaries)
}

impl<C: SegmentSource> PhaseOperation for TablePut<'_, C> {
    type Error = SegmentedPutError;

    fn op(&self) -> &'static [u8] {
        TABLE_PUT_OP
    }

    fn shape(&self) -> InstanceShapeKind {
        InstanceShapeKind::Table
    }

    fn segment_op(&self) -> SegmentOp {
        SegmentOp::Put
    }

    fn locality(&self) -> Locality {
        put_locality(self.engine.lens, self.engine.plan)
    }

    fn working_set(&self, record: &SegmentRecord) -> Result<u64, String> {
        let complement = self
            .complements
            .byte_len(record.id)
            .map_err(|error| error.to_string())?;
        Ok(MemoryModel::estimate(&record.stats).saturating_add(complement))
    }

    fn read_inputs(&self, _: &PhaseContext<'_>, task: TaskKey) -> Result<Vec<Digest>, String> {
        let (Phase::Validate, Some(segment)) = (task.phase, task.segment) else {
            return Ok(Vec::new());
        };
        let bytes = self
            .complements
            .read(segment)
            .map_err(|error| error.to_string())?;
        Ok(vec![Digest::derive(SEGMENT_COMPLEMENT_CONTEXT, &bytes)])
    }

    fn validate(
        &self,
        cx: &PhaseContext<'_>,
        segment: SegmentId,
        bytes: Vec<u8>,
    ) -> Result<Vec<u8>, PutStop> {
        let complement = self.complements.read(segment).map_err(store_failure)?;
        let decoded = TableComplementSegment::from_canonical_bytes(&complement, segment)?;
        check_complement(&decoded, cx.index())?;
        let mut sink = CanonicalWriter::new();
        sink.u64(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        sink.write(&bytes);
        sink.write(&complement);
        Ok(sink.into_bytes())
    }

    fn export(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Vec<u8>, PutStop> {
        let part = Self::validated(cx, segment)?;
        Ok(Exported::of(&part, cx.index()).bytes())
    }

    fn summarize(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Vec<u8>, PutStop> {
        let part = Self::validated(cx, segment)?;
        let mut wanted = BTreeSet::new();
        for id in cx.segments() {
            let exported = Exported::decode(&cx.artifact(Phase::Export, Some(id))?)?;
            if let Some(rows) = exported.requests.get(&segment) {
                wanted.extend(rows.iter().copied());
            }
        }
        let facts = part.offer(
            self.engine.lens,
            (self.engine.functor, self.engine.cells),
            &wanted,
        )?;
        Ok(facts_bytes(&facts))
    }

    fn assemble(&self, cx: &PhaseContext<'_>) -> Result<Vec<u8>, PutStop> {
        let exports = Self::exports(cx)?;
        let shared = Self::shared(cx, &exports, cx.segments())?;
        let ids = cx.segments();
        let mut found: Vec<PutDomainError> = Vec::new();
        let mut errors: Vec<PutDomainError> = Vec::new();
        let mut boundaries: Vec<Boundary> = vec![Boundary::default(); ids.len()];
        let mut crossings: Vec<Crossing> = Vec::new();
        for &segment in &ids {
            let restoration = self.restoration(cx, segment, &shared)?;
            found.extend(restoration.pre);
            errors.extend(restoration.errors);
            let (member, crossed) = family_segment(segment, &restoration.source, cx.index())?;
            if let Some(boundary) = boundaries.get_mut(position(segment)) {
                boundary.ghosts = member.boundary.ghosts;
            }
            crossings.extend(crossed);
        }
        if errors.is_empty() {
            for (owner, elem, stub) in crossings {
                if let Some(boundary) = boundaries.get_mut(position(owner)) {
                    boundary.in_stubs.insert(elem, stub);
                }
            }
            found.extend(self.source_clauses(cx, &shared, &boundaries)?);
        } else {
            found.extend(errors);
        }
        least(found).map_or_else(
            || Ok(boundaries_bytes(&boundaries)),
            |error| Err(refused(error)),
        )
    }

    fn emit(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Emission, PutStop> {
        let boundaries = decode_boundaries(&cx.artifact(Phase::Assemble, None)?)?;
        let exports = Self::exports(cx)?;
        let part = Self::validated(cx, segment)?;
        let owners: BTreeSet<SegmentId> = exports
            .iter()
            .find(|(id, _)| *id == segment)
            .map(|(_, exported)| exported.requests.keys().copied().collect())
            .unwrap_or_default();
        let shared = Self::shared(cx, &exports, owners)?;
        let facts: BTreeMap<RowId, RowFact> = part
            .requests(cx.index())
            .into_iter()
            .filter_map(|row| Some((row, shared.facts.get(&row)?.clone())))
            .collect();
        let restoration = self
            .engine
            .restore(&part, &facts, shared.floor, cx.index())?;
        let pulled = self.engine.pull(&restoration, cx.index())?;
        let (mut restored, _) = family_segment(segment, &pulled.source, cx.index())?;
        restored.boundary = boundaries
            .into_iter()
            .nth(position(segment))
            .ok_or_else(|| SegmentError::Decode {
                artifact: ArtifactKind::Assembly,
                message: "a boundary is missing".to_owned(),
            })?;
        Ok(Emission {
            segment: AnySegment::Table(restored),
            complement: None,
        })
    }

    fn output_role(&self, _: &PhaseContext<'_>) -> ManifestRole {
        ManifestRole::Source
    }
}

/// Backward table-lens direction over a view held as segments, as a
/// checkpointed run of [`TablePut`].
///
/// The table plan of `lens` at `protocols` is compiled once. The run's lens
/// identity is the table identity of `lens` ([`Lens::identity`] at
/// [`ComplementShape::Functor`]) and the restored segments are written
/// under its source schema digest, so the run identity is a function of the
/// lens and the view; it stops once `cancel_after_tasks` tasks are done, and
/// a later call against the same store resumes it. A complete run emits each
/// restored segment, read back with
/// [`read_emitted_segment`](panproto_inst::segment::read_emitted_segment).
///
/// Write `glue` for gluing table segments under the view's partition and
/// `join` for joining the complement segments into one complement stamped
/// with the table identity of `lens`. Then the restored segments, glued, are
/// [`put`](super::put) of `glue(views)` against `join(complements)`, and each
/// holds the restored rows its segment owns and the restored pairs whose
/// source row it owns, with the boundary those pairs cross. When whole-state
/// put refuses, the run fails at Assemble with the same refusal, the least
/// violation found in any segment, and seals nothing.
///
/// A lens [`table_put_locality`] classifies as global is declined by the run
/// with a global residual before any segment is read: a restrict carrying an
/// expression, and a source schema declaring a uniqueness or primary key.
/// A complement entry about a row its segment does not own is refused at
/// Validate with [`SegmentError::WrongOwner`] or [`SegmentError::Unowned`].
///
/// # Errors
///
/// The table plan's compilation error; [`LensError::ComplementMismatch`]
/// when the plan carries no table functor or cell plan, for which
/// whole-state put determines no restoration.
pub fn put_segmented<Src: SegmentSource, C: SegmentSource, St: SegmentStore>(
    lens: &Lens,
    protocols: BoundaryProtocols<'_>,
    view: &HeldView<'_, Src, C>,
    budget: Budget,
    store: &mut St,
    cancel_after_tasks: Option<u32>,
) -> Result<SegmentedRun<SegmentedPutError>, LensError> {
    let HeldView {
        manifest,
        input,
        views,
        complements,
    } = *view;
    let plan = compile(lens, protocols)?;
    let (Some(functor), Some(cells)) = (
        plan.boundary
            .as_ref()
            .and_then(|boundary| boundary.table.as_ref()),
        plan.boundary
            .as_ref()
            .and_then(|boundary| boundary.cells.as_ref()),
    ) else {
        return Err(LensError::ComplementMismatch {
            detail: "table restoration was not determined".to_owned(),
        });
    };
    let identity = lens.identity(ComplementShape::Functor);
    let put = TablePut {
        engine: Engine {
            lens,
            plan: &plan,
            functor,
            cells,
            protocol: protocols.src,
        },
        complements,
    };
    Ok(run_segmented(
        &put,
        manifest,
        views,
        RunSpec {
            lens: identity.lens,
            input,
            output_schema: identity.source_schema,
            cancel_after_tasks,
            reuse: None,
        },
        budget,
        store,
        &mut |_| {},
    ))
}
