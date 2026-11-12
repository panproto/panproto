//! Restricting a table held as segments, segment by segment.
//!
//! [`TableRestrict`] runs table get along a plan compiled for tables
//! ([`RestrictPlan::compile_for_tables`]) as the phases of a checkpointed
//! run ([`run_segmented`](super::run_segmented)). Rows keep their ids and a
//! pair is held by the owner of its source row, so the view is laid out
//! under the source's partition and each segment's view and complement are
//! the entries of the whole view and complement about what it owns.
//!
//! Without an expression, whether a row survives and its view table depend
//! on its source table alone, which every boundary copy of a row carries.
//! So a segment decides its own rows and pairs from its rows, pairs and
//! ghosts, running the row and pair stages of the table engine on its rows
//! and a row without cells for each ghost. Only contraction crosses
//! segments: a surviving row reaches surviving rows through dropped rows
//! of any segment. The phases are:
//!
//! - Export: each segment runs the row and pair stages on its rows and
//!   ghosts and records its first refusal, the floor above its row ids, and
//!   for each dropped row it owns that a pair from elsewhere enters, the
//!   surviving rows (with their source tables) and the dropped ghosts it
//!   reaches through dropped rows it owns;
//! - Summarize: each segment reads every Export, follows each of its
//!   surviving rows through dropped rows (its own by its pairs, any other's
//!   by that segment's entry), and records the realized pairs it owns with
//!   both rows' source tables and its least contraction refusal;
//! - Assemble: the least refusal of the row stage, then of the pair stage,
//!   then of contraction, recomputed in the segment owning the element it
//!   arose at; this is the refusal whole-state get returns;
//! - Emit: each segment's view (its surviving rows, kept and contracted
//!   pairs, a ghost for every remote target and an in-stub for every pair
//!   entering it from elsewhere) and its [`TableComplementSegment`].
//!
//! A plan whose survival or transforms carry an expression is declined with
//! [`Residual::Global`] before any task, as graph restriction is
//! ([`graph_restrict_locality`]). The same phases run in memory over a family
//! read one segment at a time ([`table_restrict_segmented_with`]), which is
//! what a segmented table put reads its restored source's view through.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use panproto_gat::Name;
use panproto_schema::{CanonicalWriter, Edge, InstanceShapeKind};

use super::body::{AnySegment, Boundary, Ghost, InStub, TableSegment};
use super::bytes::ascending;
use super::error::Residual;
use super::error::SegmentError;
use super::ids::SegmentId;
use super::manifest::ManifestRole;
use super::partition::{OwnerIndex, Partition};
use super::reader::Reader;
use super::restrict::{
    GraphComplementSegment, SegmentedRestrictError, StreamStop, graph_restrict_locality,
};
use super::runner::{Emission, PhaseContext, PhaseOperation, PhaseStop};
use super::store::{ArtifactKind, Phase};
use crate::error::RestrictError;
use crate::functor::{FInstance, Row, write_row_record};
use crate::identity::{
    CanonicalSink, ComplementFormat, ComplementShape, Digest, FUNCTOR_COMPLEMENT_FORMAT_VERSION,
    IDENTITY_ENVELOPE_VERSION, IdentityEnvelope, SEGMENT_COMPLEMENT_CONTEXT,
};
use crate::locality::{Locality, SegmentOp};
use crate::restrict_plan::{RestrictPlan, floor_above};
use crate::table_complement::{DroppedRow, TableComplement};
use crate::table_restrict::{
    TableStageKey, contract_pair, kept_pair_image, restrict_rows_and_pairs, surviving_table,
};
use crate::value::Value;
use crate::view_ref::{RowId, ViewRef};

/// The operation bytes of table restriction in a run's identity
/// ([`RunJournal::run_digest`](super::RunJournal::run_digest)).
pub const TABLE_RESTRICT_OP: &[u8] = b"table restrict";

/// The format version a table complement segment's bytes begin with.
pub const TABLE_COMPLEMENT_SEGMENT_FORMAT_VERSION: u16 = 2;

/// A source foreign-key pair or view pair: its key and its two rows.
type Pair = (Edge, RowId, RowId);

/// The complement of one segment of a segmented table restriction: the
/// entries of the whole [`TableComplement`] about the rows the segment owns
/// and the pairs whose source row it owns.
///
/// `floor` replaces the whole complement's `id_floor`: one above the
/// greatest row id the segment owns in the source, or 0 when it owns none,
/// so the greatest floor of a family is the whole complement's floor. The
/// identity envelope is the view manifest's.
#[derive(Clone, Debug, PartialEq)]
pub struct TableComplementSegment {
    /// The segment.
    pub segment: SegmentId,
    /// The dropped source rows it owns.
    pub dropped_rows: BTreeMap<RowId, DroppedRow>,
    /// The dropped cells of kept rows it owns.
    pub dropped_cells: BTreeMap<(RowId, String), Value>,
    /// The dropped pairs whose source row it owns, by source key.
    pub dropped_fk_pairs: BTreeMap<Edge, BTreeSet<(RowId, RowId)>>,
    /// The source keys of each view pair it owns that the view does not
    /// determine.
    pub original_fk_edges: BTreeMap<Pair, BTreeSet<Edge>>,
    /// The source table of each kept row it owns that the view does not
    /// determine.
    pub original_tables: BTreeMap<RowId, Name>,
    /// The source cells of kept rows it owns that the view holds with
    /// another value.
    pub original_cells: BTreeMap<(RowId, String), Value>,
    /// The pairs contraction synthesized from a row it owns.
    pub synthesized_fk_pairs: BTreeSet<Pair>,
    /// One above its greatest source row id, or 0.
    pub floor: u32,
    /// The declared source cells absent in a kept row it owns and present
    /// in its view row.
    pub original_absent_cells: BTreeSet<(RowId, String)>,
}

impl TableComplementSegment {
    /// The entries of `whole` about the rows `owns` holds and the pairs
    /// whose source row it holds, with `floor` as the segment's floor.
    #[must_use]
    pub fn of(
        whole: &TableComplement,
        segment: SegmentId,
        owns: &dyn Fn(RowId) -> bool,
        floor: u32,
    ) -> Self {
        let rows = |row: &RowId| owns(*row);
        Self {
            segment,
            dropped_rows: owned(&whole.dropped_rows, |row| rows(row)),
            dropped_cells: owned(&whole.dropped_cells, |(row, _)| rows(row)),
            dropped_fk_pairs: whole
                .dropped_fk_pairs
                .iter()
                .map(|(edge, pairs)| {
                    let pairs: BTreeSet<(RowId, RowId)> =
                        pairs.iter().filter(|(src, _)| rows(src)).copied().collect();
                    (edge.clone(), pairs)
                })
                .filter(|(_, pairs)| !pairs.is_empty())
                .collect(),
            original_fk_edges: owned(&whole.original_fk_edges, |(_, src, _)| rows(src)),
            original_tables: owned(&whole.original_tables, |row| rows(row)),
            original_cells: owned(&whole.original_cells, |(row, _)| rows(row)),
            synthesized_fk_pairs: whole
                .synthesized_fk_pairs
                .iter()
                .filter(|(_, src, _)| rows(src))
                .cloned()
                .collect(),
            floor,
            original_absent_cells: whole
                .original_absent_cells
                .iter()
                .filter(|(row, _)| rows(row))
                .cloned()
                .collect(),
        }
    }

    /// The whole complement the segments of a family hold: the union of
    /// their entries, with the greatest floor as the id floor and no
    /// identity envelope.
    #[must_use]
    pub fn glue(segments: &[Self]) -> TableComplement {
        let mut whole = TableComplement::empty();
        for segment in segments {
            whole.dropped_rows.extend(segment.dropped_rows.clone());
            whole.dropped_cells.extend(segment.dropped_cells.clone());
            for (edge, pairs) in &segment.dropped_fk_pairs {
                whole
                    .dropped_fk_pairs
                    .entry(edge.clone())
                    .or_default()
                    .extend(pairs.iter().copied());
            }
            whole
                .original_fk_edges
                .extend(segment.original_fk_edges.clone());
            whole
                .original_tables
                .extend(segment.original_tables.clone());
            whole.original_cells.extend(segment.original_cells.clone());
            whole
                .synthesized_fk_pairs
                .extend(segment.synthesized_fk_pairs.iter().cloned());
            whole
                .original_absent_cells
                .extend(segment.original_absent_cells.iter().cloned());
            whole.id_floor = whole.id_floor.max(segment.floor);
        }
        whole
    }

    /// The number of entries the complement segment retains: its dropped
    /// rows and cells, every dropped pair, every recorded source key, its
    /// original tables and cells, its synthesized pairs and its absent
    /// cells.
    #[must_use]
    pub fn element_count(&self) -> u64 {
        let count = self.dropped_rows.len()
            + self.dropped_cells.len()
            + self
                .dropped_fk_pairs
                .values()
                .map(BTreeSet::len)
                .sum::<usize>()
            + self
                .original_fk_edges
                .values()
                .map(BTreeSet::len)
                .sum::<usize>()
            + self.original_tables.len()
            + self.original_cells.len()
            + self.synthesized_fk_pairs.len()
            + self.original_absent_cells.len();
        u64::try_from(count).unwrap_or(u64::MAX)
    }

    /// The complement segment's canonical bytes.
    ///
    /// ```text
    /// version: u16 (TABLE_COMPLEMENT_SEGMENT_FORMAT_VERSION), shape: u8 (1, table)
    /// dropped rows:      count, row records                    (ascending id)
    /// dropped cells:     count, (row, column, value)           (ascending)
    /// dropped pairs:     count, (key, count, (src, tgt)...)    (ascending)
    /// original keys:     count, (key, src, tgt, count, key...) (ascending)
    /// original tables:   count, (row, table)                   (ascending)
    /// original cells:    count, (row, column, value)           (ascending)
    /// synthesized pairs: count, (key, src, tgt)                (ascending)
    /// floors:            row floor: u32, edge floor: u32 (always 0)
    /// absent cells:      count, (row, column)                  (ascending)
    /// ```
    ///
    /// A row record is the one table segments use. The segment id is not
    /// written: the manifest the complement is filed under names it.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut sink = CanonicalWriter::new();
        sink.write(&TABLE_COMPLEMENT_SEGMENT_FORMAT_VERSION.to_le_bytes());
        sink.tag(1);
        sink.len(self.dropped_rows.len());
        for (id, dropped) in &self.dropped_rows {
            write_row_record(&mut sink, *id, &dropped.table, &dropped.row);
        }
        write_cells(&mut sink, &self.dropped_cells);
        sink.len(self.dropped_fk_pairs.len());
        for (edge, pairs) in &self.dropped_fk_pairs {
            sink.edge(edge);
            sink.len(pairs.len());
            for (src, tgt) in pairs {
                sink.u32(src.0);
                sink.u32(tgt.0);
            }
        }
        sink.len(self.original_fk_edges.len());
        for ((edge, src, tgt), carriers) in &self.original_fk_edges {
            sink.edge(edge);
            sink.u32(src.0);
            sink.u32(tgt.0);
            sink.len(carriers.len());
            for carrier in carriers {
                sink.edge(carrier);
            }
        }
        sink.len(self.original_tables.len());
        for (id, table) in &self.original_tables {
            sink.u32(id.0);
            sink.name(table);
        }
        write_cells(&mut sink, &self.original_cells);
        sink.len(self.synthesized_fk_pairs.len());
        for (edge, src, tgt) in &self.synthesized_fk_pairs {
            sink.edge(edge);
            sink.u32(src.0);
            sink.u32(tgt.0);
        }
        sink.u32(self.floor);
        sink.u32(0);
        sink.len(self.original_absent_cells.len());
        for (row, column) in &self.original_absent_cells {
            sink.u32(row.0);
            sink.str(column);
        }
        sink.into_bytes()
    }

    /// The complement segment of `segment` whose canonical bytes are
    /// `bytes`: the inverse of [`TableComplementSegment::canonical_bytes`].
    ///
    /// # Errors
    ///
    /// [`SegmentError::Decode`] for the complement segment artifact when the
    /// version or shape tag is not the table's, an input ends inside a
    /// field, a count exceeds the bytes left, a map's or set's keys do not
    /// strictly ascend, the edge floor is not 0, a cell is recorded both
    /// absent and present, or bytes follow the absent cells.
    pub fn from_canonical_bytes(bytes: &[u8], segment: SegmentId) -> Result<Self, SegmentError> {
        let mut r = Reader::new(bytes, ArtifactKind::ComplementSegment);
        let version = r.u16()?;
        if version != TABLE_COMPLEMENT_SEGMENT_FORMAT_VERSION {
            return Err(r.error(format!("complement format version {version}")));
        }
        let shape = r.u8()?;
        if shape != 1 {
            return Err(r.error(format!("complement shape tag {shape}")));
        }
        let mut dropped_rows = BTreeMap::new();
        let mut last = None;
        for _ in 0..r.count(20)? {
            let (id, table, row) = r.row_record()?;
            ascending(&r, &mut last, &id)?;
            dropped_rows.insert(id, DroppedRow { table, row });
        }
        let dropped_cells = read_cells(&mut r)?;
        let mut dropped_fk_pairs = BTreeMap::new();
        let mut last = None;
        for _ in 0..r.count(20)? {
            let edge = r.edge()?;
            ascending(&r, &mut last, &edge)?;
            let mut pairs = BTreeSet::new();
            let mut last_pair = None;
            for _ in 0..r.count(8)? {
                let pair = (RowId(r.u32()?), RowId(r.u32()?));
                ascending(&r, &mut last_pair, &pair)?;
                pairs.insert(pair);
            }
            dropped_fk_pairs.insert(edge, pairs);
        }
        let mut original_fk_edges = BTreeMap::new();
        let mut last = None;
        for _ in 0..r.count(28)? {
            let key = (r.edge()?, RowId(r.u32()?), RowId(r.u32()?));
            ascending(&r, &mut last, &key)?;
            let mut carriers = BTreeSet::new();
            let mut last_carrier = None;
            for _ in 0..r.count(16)? {
                let carrier = r.edge()?;
                ascending(&r, &mut last_carrier, &carrier)?;
                carriers.insert(carrier);
            }
            original_fk_edges.insert(key, carriers);
        }
        let mut original_tables = BTreeMap::new();
        let mut last = None;
        for _ in 0..r.count(12)? {
            let id = RowId(r.u32()?);
            ascending(&r, &mut last, &id)?;
            original_tables.insert(id, r.name()?);
        }
        let original_cells = read_cells(&mut r)?;
        let mut synthesized_fk_pairs = BTreeSet::new();
        let mut last = None;
        for _ in 0..r.count(24)? {
            let pair = (r.edge()?, RowId(r.u32()?), RowId(r.u32()?));
            ascending(&r, &mut last, &pair)?;
            synthesized_fk_pairs.insert(pair);
        }
        let floor = r.u32()?;
        let edge_floor = r.u32()?;
        if edge_floor != 0 {
            return Err(r.error("table complement has a nonzero edge floor"));
        }
        let mut original_absent_cells = BTreeSet::new();
        let mut last = None;
        for _ in 0..r.count(12)? {
            let cell = (RowId(r.u32()?), r.string()?);
            ascending(&r, &mut last, &cell)?;
            original_absent_cells.insert(cell);
        }
        r.finish()?;
        let present = |cell: &(RowId, String)| {
            dropped_cells.contains_key(cell)
                || original_cells.contains_key(cell)
                || dropped_rows
                    .get(&cell.0)
                    .is_some_and(|dropped| dropped.row.contains_key(&cell.1))
        };
        if original_absent_cells.iter().any(present) {
            return Err(r.error("a source cell is recorded both absent and present"));
        }
        Ok(Self {
            segment,
            dropped_rows,
            dropped_cells,
            dropped_fk_pairs,
            original_fk_edges,
            original_tables,
            original_cells,
            synthesized_fk_pairs,
            floor,
            original_absent_cells,
        })
    }

    /// The complement segment's digest: its canonical bytes under
    /// [`SEGMENT_COMPLEMENT_CONTEXT`].
    #[must_use]
    pub fn digest(&self) -> Digest {
        Digest::derive(SEGMENT_COMPLEMENT_CONTEXT, &self.canonical_bytes())
    }
}

/// The entries of `map` whose key `keep` admits.
fn owned<K: Ord + Clone, V: Clone>(
    map: &BTreeMap<K, V>,
    keep: impl Fn(&K) -> bool,
) -> BTreeMap<K, V> {
    map.iter()
        .filter(|(key, _)| keep(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn write_cells(sink: &mut CanonicalWriter, cells: &BTreeMap<(RowId, String), Value>) {
    sink.len(cells.len());
    for ((row, column), value) in cells {
        sink.u32(row.0);
        sink.str(column);
        sink.value(value);
    }
}

fn read_cells(r: &mut Reader<'_>) -> Result<BTreeMap<(RowId, String), Value>, SegmentError> {
    let mut cells = BTreeMap::new();
    let mut last = None;
    for _ in 0..r.count(13)? {
        let key = (RowId(r.u32()?), r.string()?);
        ascending(r, &mut last, &key)?;
        cells.insert(key, r.value(0)?);
    }
    Ok(cells)
}

/// The complement segment an Emit task writes.
#[derive(Clone, Debug)]
pub enum ComplementSegment {
    /// A graph restriction's complement segment.
    Graph(GraphComplementSegment),
    /// A table restriction's complement segment.
    Table(TableComplementSegment),
}

impl ComplementSegment {
    /// Its canonical bytes.
    ///
    /// # Errors
    ///
    /// As [`GraphComplementSegment::canonical_bytes`].
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SegmentError> {
        match self {
            Self::Graph(segment) => segment.canonical_bytes(),
            Self::Table(segment) => Ok(segment.canonical_bytes()),
        }
    }

    /// The number of entries it retains.
    #[must_use]
    pub fn element_count(&self) -> u64 {
        match self {
            Self::Graph(segment) => segment.element_count(),
            Self::Table(segment) => segment.element_count(),
        }
    }
}

/// Table restriction along a plan compiled for tables as a
/// [`PhaseOperation`]: the phases described in the module documentation.
pub struct TableRestrict<'p> {
    /// The compiled plan, which must carry the table functor.
    pub plan: &'p RestrictPlan,
}

/// The first refusal of one segment's row and pair stages.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Refused {
    /// No refusal.
    None,
    /// A refusal at this element.
    At(TableStageKey),
}

/// What one surviving or dropped row reaches through the dropped rows its
/// segment owns: the surviving rows, with their source tables, and the
/// dropped rows other segments own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Reach {
    survivors: BTreeMap<RowId, Name>,
    exits: BTreeSet<RowId>,
}

/// One segment's Export artifact.
#[derive(Clone, Debug)]
struct Export {
    refused: Refused,
    floor: u32,
    entries: BTreeMap<RowId, Reach>,
}

/// One segment's Summarize artifact: the realized pairs whose source row it
/// owns, with both rows' source tables, and the least of them whose
/// contraction refuses; `None` when some segment's export refused.
#[derive(Clone, Debug, Default)]
struct Realized {
    pairs: BTreeMap<(RowId, RowId), (Name, Name)>,
    refused: Option<(RowId, RowId)>,
}

/// One segment's rows as the table engine reads them: its rows, a row
/// without cells for each ghost, and its pairs.
fn local_instance(held: &TableSegment) -> Result<FInstance, SegmentError> {
    let mut by_table: BTreeMap<Name, Vec<(RowId, Row)>> = BTreeMap::new();
    for (id, (table, row)) in &held.rows {
        by_table
            .entry(table.clone())
            .or_default()
            .push((*id, row.clone()));
    }
    for ghost in held.boundary.ghosts.values() {
        if !held.rows.contains_key(&RowId(ghost.node)) {
            by_table
                .entry(ghost.anchor.clone())
                .or_default()
                .push((RowId(ghost.node), Row::new()));
        }
    }
    let mut by_edge: BTreeMap<Edge, Vec<(RowId, RowId)>> = BTreeMap::new();
    for (edge, src, tgt) in &held.pairs {
        by_edge.entry(edge.clone()).or_default().push((*src, *tgt));
    }
    FInstance::from_parts(by_table, by_edge).map_err(|error| SegmentError::Decode {
        artifact: ArtifactKind::Segment,
        message: format!("a table segment does not form a table: {error}"),
    })
}

/// The source table of `row` as `held` holds it: its own row's, or its
/// ghost's anchor.
fn held_table(held: &TableSegment, row: RowId) -> Option<&Name> {
    held.rows
        .get(&row)
        .map(|(table, _)| table)
        .or_else(|| held.boundary.ghosts.get(&row.0).map(|ghost| &ghost.anchor))
}

fn malformed(artifact: ArtifactKind, message: &str) -> SegmentError {
    SegmentError::Decode {
        artifact,
        message: message.to_owned(),
    }
}

const fn restrict_error(segment: SegmentId, error: RestrictError) -> SegmentedRestrictError {
    SegmentedRestrictError::Restrict { segment, error }
}

type Stop = PhaseStop<SegmentedRestrictError>;

/// The refusal of a family that its Exports and Summaries locate, before it
/// is recomputed in the segment owning its element.
enum Located {
    /// The least refusal of the row and pair stages, at this element.
    Stage(TableStageKey),
    /// The least realized pair whose contraction refuses, with its rows'
    /// source tables.
    Contraction((RowId, RowId), (Name, Name)),
}

impl TableRestrict<'_> {
    fn validated(cx: &PhaseContext<'_>, segment: SegmentId) -> Result<TableSegment, Stop> {
        let bytes = cx.artifact(Phase::Validate, Some(segment))?;
        match AnySegment::from_canonical_bytes(&bytes, cx.partition())?.0 {
            AnySegment::Table(segment) => Ok(segment),
            _ => Err(PhaseStop::from(malformed(
                ArtifactKind::Segment,
                "not a table segment",
            ))),
        }
    }

    fn exported(cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Export, Stop> {
        Ok(decode_export(&cx.artifact(Phase::Export, Some(segment))?)?)
    }

    fn realized(cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Option<Realized>, Stop> {
        Ok(decode_realized(
            &cx.artifact(Phase::Summarize, Some(segment))?,
        )?)
    }

    /// Whether a row of source table `table` survives.
    fn survives(&self, table: &Name) -> bool {
        surviving_table(self.plan, table).is_some()
    }

    /// What `from` reaches in `held` through the dropped rows `held` owns,
    /// starting at the targets of its pairs (the first step's surviving
    /// targets are not reached through a dropped row and are left out when
    /// `from` itself survives).
    fn local_reach(&self, held: &TableSegment, from: RowId, from_survives: bool) -> Reach {
        let mut out: BTreeMap<RowId, Vec<RowId>> = BTreeMap::new();
        for (_, src, tgt) in &held.pairs {
            out.entry(*src).or_default().push(*tgt);
        }
        let mut reach = Reach::default();
        let mut visited: BTreeSet<RowId> = BTreeSet::new();
        let mut queue: VecDeque<(RowId, bool)> = VecDeque::from([(from, from_survives)]);
        while let Some((at, first)) = queue.pop_front() {
            for next in out.get(&at).map_or(&[][..], Vec::as_slice) {
                let Some(table) = held_table(held, *next) else {
                    continue;
                };
                if self.survives(table) {
                    if !first {
                        reach.survivors.insert(*next, table.clone());
                    }
                } else if visited.insert(*next) {
                    if held.rows.contains_key(next) {
                        queue.push_back((*next, false));
                    } else {
                        reach.exits.insert(*next);
                    }
                }
            }
        }
        reach
    }

    /// The Export of `held`: the first refusal of its row and pair stages,
    /// its floor, and the reach of each dropped row it owns that a pair from
    /// another segment enters.
    fn export_of(&self, held: &TableSegment) -> Result<Export, SegmentedRestrictError> {
        let refused = match restrict_rows_and_pairs(&local_instance(held)?, self.plan) {
            Ok(_) => Refused::None,
            Err(refused) => match *refused {
                (Some(key), _) => Refused::At(key),
                (None, error) => return Err(restrict_error(held.segment, error)),
            },
        };
        let floor = held
            .rows
            .keys()
            .next_back()
            .map_or(0, |last| floor_above(Some(last.0)));
        let mut entries = BTreeMap::new();
        for stub in held.boundary.in_stubs.values() {
            let tgt = RowId(stub.tgt);
            let Some((table, _)) = held.rows.get(&tgt) else {
                continue;
            };
            if !self.survives(table) && !entries.contains_key(&tgt) {
                entries.insert(tgt, self.local_reach(held, tgt, false));
            }
        }
        Ok(Export {
            refused,
            floor,
            entries,
        })
    }

    /// The realized pairs whose source row `held` owns, with both rows'
    /// source tables, followed through the entries `entries` of every
    /// segment, and the least of them whose contraction refuses.
    fn realized_of(
        &self,
        held: &TableSegment,
        entries: &BTreeMap<RowId, Reach>,
    ) -> Result<Realized, SegmentError> {
        let mut realized = Realized::default();
        for (src, (src_table, _)) in &held.rows {
            if !self.survives(src_table) {
                continue;
            }
            let start = self.local_reach(held, *src, true);
            let mut reached = start.survivors;
            let mut visited: BTreeSet<RowId> = start.exits.clone();
            let mut queue: VecDeque<RowId> = start.exits.into_iter().collect();
            while let Some(at) = queue.pop_front() {
                let entry = if held.rows.contains_key(&at) {
                    self.local_reach(held, at, false)
                } else {
                    entries.get(&at).cloned().ok_or_else(|| {
                        malformed(ArtifactKind::Export, "a dropped row entered has no entry")
                    })?
                };
                reached.extend(entry.survivors);
                for exit in entry.exits {
                    if visited.insert(exit) {
                        queue.push_back(exit);
                    }
                }
            }
            for (tgt, tgt_table) in reached {
                realized
                    .pairs
                    .insert((*src, tgt), (src_table.clone(), tgt_table));
            }
        }
        let mut probe = (FInstance::new(), TableComplement::empty());
        realized.refused = realized
            .pairs
            .iter()
            .find(|((src, tgt), (a, b))| {
                contract_pair(
                    self.plan,
                    (&mut probe.0, &mut probe.1),
                    (*src, a),
                    (*tgt, b),
                )
                .is_err()
            })
            .map(|(pair, _)| *pair);
        Ok(realized)
    }

    /// The least stage refusal among the segments' exports.
    fn least_stage<'e>(refusals: impl IntoIterator<Item = &'e Refused>) -> Option<TableStageKey> {
        let mut least: Option<TableStageKey> = None;
        for refused in refusals {
            if let Refused::At(key) = refused
                && least.as_ref().is_none_or(|least| key < least)
            {
                least = Some(key.clone());
            }
        }
        least
    }

    /// The least refused contraction among the segments' realized pairs.
    fn least_contraction<'r>(
        realized: impl IntoIterator<Item = &'r Realized>,
    ) -> Result<Option<Located>, SegmentError> {
        let mut contracted: Option<((RowId, RowId), (Name, Name))> = None;
        for realized in realized {
            if let Some(pair) = realized.refused
                && contracted.as_ref().is_none_or(|(least, _)| pair < *least)
            {
                let tables =
                    realized.pairs.get(&pair).cloned().ok_or_else(|| {
                        malformed(ArtifactKind::Summary, "a refused pair is missing")
                    })?;
                contracted = Some((pair, tables));
            }
        }
        Ok(contracted.map(|(pair, tables)| Located::Contraction(pair, tables)))
    }

    /// The segment owning the element a located refusal arose at.
    fn owner_of(located: &Located, index: &OwnerIndex) -> Result<SegmentId, SegmentError> {
        let row = match located {
            Located::Stage(TableStageKey::Row(row) | TableStageKey::Pair(_, row, _))
            | Located::Contraction((row, _), _) => *row,
        };
        index
            .owner_of_node(row.0)
            .ok_or_else(|| malformed(ArtifactKind::Export, "an unowned row"))
    }

    /// The refusal of a stage refusal located at `key`, recomputed in
    /// `held`, the segment owning its element: the error whole-state get
    /// returns.
    fn confirm_stage(&self, key: &TableStageKey, held: &TableSegment) -> SegmentedRestrictError {
        let instance = match local_instance(held) {
            Ok(instance) => instance,
            Err(error) => return error.into(),
        };
        match restrict_rows_and_pairs(&instance, self.plan).map_err(|refused| *refused) {
            Err((Some(found), error)) if found == *key => restrict_error(held.segment, error),
            _ => SegmentedRestrictError::Segment(malformed(
                ArtifactKind::Export,
                "an export records a refusal its segment does not give",
            )),
        }
    }

    /// The refusal of contracting the pair `(src, tgt)` of source tables
    /// `(a, b)`, in `at`, the segment owning its source row.
    fn confirm_contraction(
        &self,
        at: SegmentId,
        (src, tgt): (RowId, RowId),
        (a, b): &(Name, Name),
    ) -> SegmentedRestrictError {
        let mut probe = (FInstance::new(), TableComplement::empty());
        contract_pair(self.plan, (&mut probe.0, &mut probe.1), (src, a), (tgt, b))
            .err()
            .map_or_else(
                || {
                    SegmentedRestrictError::Segment(malformed(
                        ArtifactKind::Summary,
                        "a summary records a contraction that does not refuse",
                    ))
                },
                |error| restrict_error(at, error),
            )
    }

    /// The view segment of `held` and its complement segment, with the kept
    /// pairs entering it from its source in-stubs; the contracted pairs
    /// entering it from other segments are added by [`Self::contracted_into`].
    fn emit_of(
        &self,
        held: &TableSegment,
        index: &OwnerIndex,
        realized: &Realized,
        floor: u32,
    ) -> Result<(TableSegment, TableComplementSegment), SegmentedRestrictError> {
        let segment = held.segment;
        let here = |row: RowId| index.owner_of_node(row.0) == Some(segment);
        let (mut view, mut complement) = restrict_rows_and_pairs(&local_instance(held)?, self.plan)
            .map_err(|refused| restrict_error(segment, refused.1))?;
        for ((src, tgt), (a, b)) in &realized.pairs {
            contract_pair(
                self.plan,
                (&mut view, &mut complement),
                (*src, a),
                (*tgt, b),
            )
            .map_err(|error| restrict_error(segment, error))?;
        }
        let complement = TableComplementSegment::of(&complement, segment, &here, floor);
        let view_tables: BTreeMap<RowId, Name> = realized
            .pairs
            .iter()
            .filter_map(|((_, tgt), (_, b))| {
                surviving_table(self.plan, b).map(|table| (*tgt, table.clone()))
            })
            .collect();
        let mut rows = BTreeMap::new();
        for id in view.row_ids() {
            if let (true, Some(table), Some(row)) = (here(id), view.table_of(id), view.row(id)) {
                rows.insert(id, (table.clone(), row.clone()));
            }
        }
        let pairs: BTreeSet<Pair> = view.all_pairs().into_iter().collect();
        let mut boundary = Boundary::default();
        for (_, _, tgt) in &pairs {
            if here(*tgt) {
                continue;
            }
            let owner = index
                .owner_of_node(tgt.0)
                .ok_or_else(|| malformed(ArtifactKind::Segment, "an unowned row"))?;
            let anchor = view
                .table_of(*tgt)
                .or_else(|| view_tables.get(tgt))
                .cloned()
                .ok_or_else(|| malformed(ArtifactKind::Segment, "a target has no table"))?;
            boundary.ghosts.entry(tgt.0).or_insert(Ghost {
                node: tgt.0,
                owner,
                anchor,
            });
        }
        for ((edge, _, _), stub) in
            held.boundary
                .in_stubs
                .iter()
                .filter_map(|(elem, stub)| match elem {
                    ViewRef::Pair { edge, src, tgt } => Some(((edge, src, tgt), stub)),
                    _ => None,
                })
        {
            if let (Some(src_table), Some(image), true) = (
                surviving_table(self.plan, &stub.src_anchor),
                kept_pair_image(self.plan, edge),
                rows.contains_key(&RowId(stub.tgt)),
            ) {
                insert_stub(
                    &mut boundary,
                    image,
                    (stub.src, stub.src_owner, src_table.clone()),
                    stub.tgt,
                );
            }
        }
        Ok((
            TableSegment {
                segment,
                rows,
                pairs,
                boundary,
            },
            complement,
        ))
    }

    /// Add to `view`'s boundary an in-stub for every contracted pair the
    /// realized pairs `other` of segment `id` place on a row `view` owns.
    fn contracted_into(
        &self,
        view: &mut TableSegment,
        index: &OwnerIndex,
        id: SegmentId,
        other: &Realized,
    ) -> Result<(), SegmentedRestrictError> {
        for ((src, tgt), (a, b)) in &other.pairs {
            if index.owner_of_node(tgt.0) != Some(view.segment) {
                continue;
            }
            let mut probe = (FInstance::new(), TableComplement::empty());
            contract_pair(
                self.plan,
                (&mut probe.0, &mut probe.1),
                (*src, a),
                (*tgt, b),
            )
            .map_err(|error| restrict_error(id, error))?;
            let (label, src_table) = probe
                .1
                .synthesized_fk_pairs
                .iter()
                .next()
                .map(|(edge, _, _)| edge.clone())
                .zip(surviving_table(self.plan, a).cloned())
                .ok_or_else(|| malformed(ArtifactKind::Summary, "a contracted pair"))?;
            insert_stub(&mut view.boundary, label, (src.0, id, src_table), tgt.0);
        }
        Ok(())
    }
}

/// Add the in-stub of the pair `label` from the row `src` (owned by
/// `src_owner`, of view table `src_table`) to the row `tgt`.
fn insert_stub(
    boundary: &mut Boundary,
    label: Edge,
    (src, src_owner, src_table): (u32, SegmentId, Name),
    tgt: u32,
) {
    boundary.in_stubs.insert(
        ViewRef::Pair {
            edge: label.clone(),
            src: RowId(src),
            tgt: RowId(tgt),
        },
        InStub {
            src,
            src_owner,
            src_anchor: src_table,
            tgt,
            label,
        },
    );
}

/// A caller's source of the segments of a table family, one at a time.
pub type FetchTableSegment<'f, 's, E> =
    dyn FnMut(SegmentId) -> Result<Cow<'s, TableSegment>, E> + 'f;

/// Table restriction along `plan` of the table held as `segments` under
/// `partition`: each segment's view and complement segment, in id order.
///
/// Glued, the views are [`functor_restrict_finite_with_plan`] of the glued
/// table and the complements its complement, without an identity stamp; a
/// refusal is the refusal whole-state get returns, in the segment owning
/// the element it arose at. This runs the phases of [`TableRestrict`] in
/// memory, by [`table_restrict_segmented_with`].
///
/// [`functor_restrict_finite_with_plan`]: crate::functor_restrict_finite_with_plan
///
/// # Errors
///
/// [`SegmentedRestrictError::Residual`] with [`Residual::Global`] when
/// [`graph_restrict_locality`] classifies the plan as global, before any
/// segment is read; [`SegmentedRestrictError::Segment`] with the error of
/// [`verify_partition`](super::verify_partition); otherwise the refusal of
/// [`table_restrict_segmented_with`].
pub fn table_restrict_segmented(
    plan: &RestrictPlan,
    segments: &[TableSegment],
    partition: &Partition,
) -> Result<Vec<(TableSegment, TableComplementSegment)>, SegmentedRestrictError> {
    if let Locality::Global(reason) = graph_restrict_locality(plan) {
        return Err(SegmentedRestrictError::Residual(Residual::Global {
            op: SegmentOp::Restrict,
            reason,
        }));
    }
    let any: Vec<AnySegment> = segments.iter().cloned().map(AnySegment::Table).collect();
    super::check::verify_partition(&any, partition)?;
    let index = partition.owner_index()?;
    let ids: Vec<SegmentId> = segments.iter().map(|segment| segment.segment).collect();
    let position: BTreeMap<SegmentId, usize> =
        ids.iter().enumerate().map(|(at, id)| (*id, at)).collect();
    let mut out = Vec::with_capacity(segments.len());
    let outcome = table_restrict_segmented_with::<SegmentError>(
        plan,
        &ids,
        &index,
        &mut |id| {
            position
                .get(&id)
                .and_then(|at| segments.get(*at))
                .map(Cow::Borrowed)
                .ok_or(SegmentError::MalformedPartition {
                    segment: id,
                    defect: super::error::PartitionDefect::SegmentMissing,
                })
        },
        &mut |_, view, complement| {
            out.push((view, complement));
            Ok(())
        },
    );
    match outcome {
        Ok(()) => Ok(out),
        Err(StreamStop::Refused(error)) => Err(error),
        Err(StreamStop::Source(error)) => Err(SegmentedRestrictError::Segment(error)),
    }
}

/// Run the phases of [`TableRestrict`] over a table family read one
/// segment at a time.
///
/// The family's segments are those `ids` names, in id order, owned under
/// `index`. Each segment is read from `fetch` whenever a pass reaches it,
/// never two at once: the Export pass, the Summarize pass, the segment
/// owning a refusal, and the Emit pass, which gives `visit` each segment's
/// view and complement segment. Between passes the run holds every
/// segment's export (its refusal, floor and entry summaries, which grow
/// with the boundary) and its realized pairs (which grow with the
/// contracted pairs). The caller must supply a family
/// [`verify_partition`](super::verify_partition) accepts and the same
/// segment each time one is fetched; the plan's locality is the caller's to
/// check.
///
/// # Errors
///
/// [`StreamStop::Source`] with the first error of `fetch` or `visit`;
/// [`StreamStop::Refused`] with [`SegmentedRestrictError::Segment`] when a
/// segment does not form a table, and otherwise
/// [`SegmentedRestrictError::Restrict`] with the refusal whole-state get
/// returns, in the segment owning the element it arose at: the least
/// refusal of the row stage, then of the pair stage, then of contraction.
pub fn table_restrict_segmented_with<E>(
    plan: &RestrictPlan,
    ids: &[SegmentId],
    index: &OwnerIndex,
    fetch: &mut FetchTableSegment<'_, '_, E>,
    visit: &mut dyn FnMut(SegmentId, TableSegment, TableComplementSegment) -> Result<(), E>,
) -> Result<(), StreamStop<E>> {
    let op = TableRestrict { plan };
    let mut exports: Vec<Export> = Vec::with_capacity(ids.len());
    for &id in ids {
        let held = fetch(id).map_err(StreamStop::Source)?;
        exports.push(op.export_of(&held).map_err(StreamStop::Refused)?);
    }
    let located = TableRestrict::least_stage(exports.iter().map(|export| &export.refused))
        .map(Located::Stage);
    let mut realized: Vec<Realized> = Vec::new();
    let located = if located.is_some() {
        located
    } else {
        let entries: BTreeMap<RowId, Reach> = exports
            .iter()
            .flat_map(|export| export.entries.clone())
            .collect();
        for &id in ids {
            let held = fetch(id).map_err(StreamStop::Source)?;
            realized.push(
                op.realized_of(&held, &entries)
                    .map_err(|error| StreamStop::Refused(error.into()))?,
            );
        }
        TableRestrict::least_contraction(&realized)
            .map_err(|error| StreamStop::Refused(error.into()))?
    };
    if let Some(located) = located {
        let at = TableRestrict::owner_of(&located, index)
            .map_err(|error| StreamStop::Refused(error.into()))?;
        return Err(StreamStop::Refused(match &located {
            Located::Stage(key) => {
                let held = fetch(at).map_err(StreamStop::Source)?;
                op.confirm_stage(key, &held)
            }
            Located::Contraction(pair, tables) => op.confirm_contraction(at, *pair, tables),
        }));
    }
    for ((&id, export), mine) in ids.iter().zip(&exports).zip(&realized) {
        let held = fetch(id).map_err(StreamStop::Source)?;
        let (mut view, complement) = op
            .emit_of(&held, index, mine, export.floor)
            .map_err(StreamStop::Refused)?;
        for (&other, theirs) in ids.iter().zip(&realized) {
            if other != id {
                op.contracted_into(&mut view, index, other, theirs)
                    .map_err(StreamStop::Refused)?;
            }
        }
        visit(id, view, complement).map_err(StreamStop::Source)?;
    }
    Ok(())
}

impl PhaseOperation for TableRestrict<'_> {
    type Error = SegmentedRestrictError;

    fn op(&self) -> &'static [u8] {
        TABLE_RESTRICT_OP
    }

    fn shape(&self) -> InstanceShapeKind {
        InstanceShapeKind::Table
    }

    fn segment_op(&self) -> SegmentOp {
        SegmentOp::Restrict
    }

    fn locality(&self) -> Locality {
        graph_restrict_locality(self.plan)
    }

    fn portable(&self, phase: Phase) -> bool {
        phase != Phase::Seal
    }

    fn export(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Vec<u8>, Stop> {
        let held = Self::validated(cx, segment)?;
        let export = self.export_of(&held).map_err(PhaseStop::Failed)?;
        Ok(export_bytes(&export))
    }

    fn summarize(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Vec<u8>, Stop> {
        let mut entries: BTreeMap<RowId, Reach> = BTreeMap::new();
        for id in cx.segments() {
            let export = Self::exported(cx, id)?;
            if export.refused != Refused::None {
                return Ok(realized_bytes(None));
            }
            entries.extend(export.entries);
        }
        let held = Self::validated(cx, segment)?;
        Ok(realized_bytes(Some(&self.realized_of(&held, &entries)?)))
    }

    fn assemble(&self, cx: &PhaseContext<'_>) -> Result<Vec<u8>, Stop> {
        let mut refusals = Vec::new();
        for id in cx.segments() {
            refusals.push(Self::exported(cx, id)?.refused);
        }
        let located = if let Some(key) = Self::least_stage(&refusals) {
            Some(Located::Stage(key))
        } else {
            let mut realized = Vec::new();
            for id in cx.segments() {
                realized.push(Self::realized(cx, id)?.ok_or_else(|| {
                    PhaseStop::from(malformed(ArtifactKind::Summary, "a skipped summary"))
                })?);
            }
            Self::least_contraction(&realized)?
        };
        let Some(located) = located else {
            return Ok(Vec::new());
        };
        let at = Self::owner_of(&located, cx.index())?;
        Err(PhaseStop::Failed(match &located {
            Located::Stage(key) => self.confirm_stage(key, &Self::validated(cx, at)?),
            Located::Contraction(pair, tables) => self.confirm_contraction(at, *pair, tables),
        }))
    }

    fn emit(&self, cx: &PhaseContext<'_>, segment: SegmentId) -> Result<Emission, Stop> {
        let held = Self::validated(cx, segment)?;
        let realized = Self::realized(cx, segment)?.ok_or_else(|| {
            PhaseStop::from(malformed(ArtifactKind::Summary, "a skipped summary"))
        })?;
        let export = Self::exported(cx, segment)?;
        let (mut view, complement) = self
            .emit_of(&held, cx.index(), &realized, export.floor)
            .map_err(PhaseStop::Failed)?;
        for id in cx.segments().into_iter().filter(|id| *id != segment) {
            let Some(other) = Self::realized(cx, id)? else {
                continue;
            };
            self.contracted_into(&mut view, cx.index(), id, &other)
                .map_err(PhaseStop::Failed)?;
        }
        Ok(Emission {
            segment: AnySegment::Table(view),
            complement: Some(ComplementSegment::Table(complement)),
        })
    }

    fn output_role(&self, cx: &PhaseContext<'_>) -> ManifestRole {
        ManifestRole::View {
            source: cx.spec().input,
            envelope: IdentityEnvelope {
                version: IDENTITY_ENVELOPE_VERSION,
                source_schema: cx.input().schema,
                target_schema: cx.spec().output_schema,
                lens: cx.spec().lens,
                complement_format: ComplementFormat {
                    shape: ComplementShape::Functor,
                    version: FUNCTOR_COMPLEMENT_FORMAT_VERSION,
                },
            },
        }
    }
}

fn write_key(sink: &mut CanonicalWriter, key: &TableStageKey) {
    match key {
        TableStageKey::Row(row) => {
            sink.tag(1);
            sink.u32(row.0);
        }
        TableStageKey::Pair(edge, src, tgt) => {
            sink.tag(2);
            sink.edge(edge);
            sink.u32(src.0);
            sink.u32(tgt.0);
        }
    }
}

fn read_key(r: &mut Reader<'_>) -> Result<Refused, SegmentError> {
    match r.u8()? {
        0 => Ok(Refused::None),
        1 => Ok(Refused::At(TableStageKey::Row(RowId(r.u32()?)))),
        2 => Ok(Refused::At(TableStageKey::Pair(
            r.edge()?,
            RowId(r.u32()?),
            RowId(r.u32()?),
        ))),
        other => Err(r.error(format!("refusal tag {other}"))),
    }
}

fn write_tabled(sink: &mut CanonicalWriter, rows: &BTreeMap<RowId, Name>) {
    sink.len(rows.len());
    for (row, table) in rows {
        sink.u32(row.0);
        sink.name(table);
    }
}

fn read_tabled(r: &mut Reader<'_>) -> Result<BTreeMap<RowId, Name>, SegmentError> {
    let mut rows = BTreeMap::new();
    let mut last = None;
    for _ in 0..r.count(12)? {
        let row = RowId(r.u32()?);
        ascending(r, &mut last, &row)?;
        rows.insert(row, r.name()?);
    }
    Ok(rows)
}

/// An Export artifact: the refusal (tag 0 none, 1 a row, 2 a pair), the
/// floor, then each entry's row, surviving rows with tables and exits.
fn export_bytes(export: &Export) -> Vec<u8> {
    let mut sink = CanonicalWriter::new();
    match &export.refused {
        Refused::None => sink.tag(0),
        Refused::At(key) => write_key(&mut sink, key),
    }
    sink.u32(export.floor);
    sink.len(export.entries.len());
    for (row, reach) in &export.entries {
        sink.u32(row.0);
        write_tabled(&mut sink, &reach.survivors);
        sink.len(reach.exits.len());
        for exit in &reach.exits {
            sink.u32(exit.0);
        }
    }
    sink.into_bytes()
}

fn decode_export(bytes: &[u8]) -> Result<Export, SegmentError> {
    let mut r = Reader::new(bytes, ArtifactKind::Export);
    let refused = read_key(&mut r)?;
    let floor = r.u32()?;
    let mut entries = BTreeMap::new();
    let mut last = None;
    for _ in 0..r.count(12)? {
        let row = RowId(r.u32()?);
        ascending(&r, &mut last, &row)?;
        let survivors = read_tabled(&mut r)?;
        let mut exits = BTreeSet::new();
        let mut last_exit = None;
        for _ in 0..r.count(4)? {
            let exit = RowId(r.u32()?);
            ascending(&r, &mut last_exit, &exit)?;
            exits.insert(exit);
        }
        entries.insert(row, Reach { survivors, exits });
    }
    r.finish()?;
    Ok(Export {
        refused,
        floor,
        entries,
    })
}

/// A Summarize artifact: tag 0 when skipped; otherwise tag 1, the realized
/// pairs with both source tables, and the refused pair (tag 0 or 1 and the
/// pair).
fn realized_bytes(realized: Option<&Realized>) -> Vec<u8> {
    let mut sink = CanonicalWriter::new();
    let Some(realized) = realized else {
        sink.tag(0);
        return sink.into_bytes();
    };
    sink.tag(1);
    sink.len(realized.pairs.len());
    for ((src, tgt), (a, b)) in &realized.pairs {
        sink.u32(src.0);
        sink.u32(tgt.0);
        sink.name(a);
        sink.name(b);
    }
    match realized.refused {
        None => sink.tag(0),
        Some((src, tgt)) => {
            sink.tag(1);
            sink.u32(src.0);
            sink.u32(tgt.0);
        }
    }
    sink.into_bytes()
}

fn decode_realized(bytes: &[u8]) -> Result<Option<Realized>, SegmentError> {
    let mut r = Reader::new(bytes, ArtifactKind::Summary);
    if r.u8()? == 0 {
        r.finish()?;
        return Ok(None);
    }
    let mut pairs = BTreeMap::new();
    let mut last = None;
    for _ in 0..r.count(16)? {
        let pair = (RowId(r.u32()?), RowId(r.u32()?));
        ascending(&r, &mut last, &pair)?;
        pairs.insert(pair, (r.name()?, r.name()?));
    }
    let refused = match r.u8()? {
        0 => None,
        1 => Some((RowId(r.u32()?), RowId(r.u32()?))),
        other => return Err(r.error(format!("refused pair tag {other}"))),
    };
    r.finish()?;
    Ok(Some(Realized { pairs, refused }))
}
