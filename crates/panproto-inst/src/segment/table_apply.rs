//! Applying a table delta to a table held as segments.
//!
//! A [`TableDelta`] names every row and pair it deletes, adds or changes.
//! Its read set ([`SegmentDelta::read_set`]) is the owner of every row it
//! names ([`TableDelta::touched_rows`]: the rows it deletes, adds or
//! changes and the endpoints of the pairs it deletes or adds). A pair has
//! no id of its own and is held by the owner of its source row, so no
//! segment beyond the rows' owners can hold what the delta names.
//!
//! A table change needs no boundary peer, unlike an anchor change of a
//! graph node. A pair joins its edge's endpoint tables before the delta,
//! and check 6 of [`TableDelta::check`] asks every pair of the result to
//! join them after it, so a pair kept at a row whose table changes is
//! refused: no table holds both the old and the new table at that end. An
//! applied table change therefore leaves no ghost or in-stub copying the
//! row's old table, and every copy of its new table belongs to a pair the
//! delta adds, whose endpoints' owners are read.
//!
//! Each segment it reads reports its facts: the rows it owns that the delta
//! names; the pairs it holds that the delta deletes or adds, and those
//! incident to a row it owns that the delta deletes or moves to another
//! table; the in-stubs entering such a row; and the table, as the segment
//! holds it, of every endpoint of those pairs and in-stubs that is not a
//! reported row. These are exactly what [`TableDelta::check`] reads of the
//! whole table: the joins of check 6 read the remote endpoint's table, which
//! the segment's ghost or in-stub carries.
//!
//! An added row no range owns is refused ([`SegmentError::Unowned`]), as
//! splitting the edited table under the partition would refuse it. Rows
//! draw fresh ids from the node allocation range of their owner
//! ([`FreshIds::node`](super::FreshIds::node)); a table has no edge ids.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::{CanonicalWriter, Edge, InstanceShapeKind};

use super::apply::{FamilyIndex, SegmentDelta, SegmentedApplyError};
use super::body::{AnySegment, Boundary, Ghost, InStub, TableSegment};
use super::bytes::ascending;
use super::error::SegmentError;
use super::ids::SegmentId;
use super::reader::Reader;
use super::store::ArtifactKind;
use crate::edit_error::EditError;
use crate::functor::{FInstance, Row, write_pair_record, write_row_record};
use crate::identity::Digest;
use crate::table_delta::{Pair, TableDelta};
use crate::view_ref::{RowId, ViewRef};

/// The operation bytes of table delta application in a run's identity
/// ([`RunJournal::run_digest`](super::RunJournal::run_digest)).
pub const TABLE_APPLY_OP: &[u8] = b"table apply";

impl SegmentDelta for TableDelta {
    type Segment = TableSegment;

    const SHAPE: InstanceShapeKind = InstanceShapeKind::Table;

    const OP: &'static [u8] = TABLE_APPLY_OP;

    fn digest(&self) -> Digest {
        Self::digest(self)
    }

    /// The owner of every row of [`TableDelta::touched_rows`]. A row no
    /// range owns contributes no segment.
    fn read_set(&self, family: &FamilyIndex) -> BTreeSet<SegmentId> {
        self.touched_rows()
            .into_iter()
            .filter_map(|row| family.owner(row.0))
            .collect()
    }

    fn facts(&self, held: &TableSegment) -> Vec<u8> {
        Facts::of(held, self).bytes()
    }

    fn check_facts(
        &self,
        facts: &[Vec<u8>],
        family: &FamilyIndex,
    ) -> Result<(), SegmentedApplyError> {
        let facts = decode_all(facts)?;
        let tables = facts_table(&facts).map_err(SegmentedApplyError::Edit)?;
        Self::check(self, &tables).map_err(SegmentedApplyError::Edit)?;
        if let Some(row) = self
            .add_rows()
            .keys()
            .find(|row| family.owner(row.0).is_none())
        {
            return Err(SegmentedApplyError::Segment(SegmentError::Unowned {
                elem: ViewRef::Row(*row),
            }));
        }
        Ok(())
    }

    fn edit(
        &self,
        held: &TableSegment,
        facts: &[Vec<u8>],
        family: &FamilyIndex,
    ) -> Result<TableSegment, SegmentError> {
        let anchors = anchors_after(self, &decode_all(facts)?);
        edit_segment(held, self, family, &anchors)
    }

    fn id(segment: &TableSegment) -> SegmentId {
        segment.segment
    }

    fn into_any(segment: TableSegment) -> AnySegment {
        AnySegment::Table(segment)
    }

    fn from_any(segment: AnySegment) -> Option<TableSegment> {
        match segment {
            AnySegment::Table(segment) => Some(segment),
            _ => None,
        }
    }
}

/// What one segment reports to the coordinator about a table delta.
///
/// `rows` holds the rows the segment owns that the delta names; `pairs` the
/// pairs it holds that the delta deletes or adds, and every pair it holds
/// incident to a row it owns that the delta deletes or whose table it
/// changes; `stubs` every in-stub entering such a row, as its pair;
/// `tables` the table, as the segment holds it, of every endpoint of those
/// pairs and stubs that `rows` does not hold.
#[derive(Clone, Debug, Default)]
struct Facts {
    rows: BTreeMap<RowId, (Name, Row)>,
    pairs: BTreeSet<Pair>,
    stubs: BTreeSet<Pair>,
    tables: BTreeMap<RowId, Name>,
}

/// The table of `row` as `held` holds it: its own row's table, the anchor
/// of its ghost, or the source anchor of an in-stub leaving it.
fn held_table(held: &TableSegment, row: RowId) -> Option<&Name> {
    held.rows
        .get(&row)
        .map(|(table, _)| table)
        .or_else(|| held.boundary.ghosts.get(&row.0).map(|ghost| &ghost.anchor))
        .or_else(|| {
            held.boundary
                .in_stubs
                .values()
                .find(|stub| stub.src == row.0)
                .map(|stub| &stub.src_anchor)
        })
}

impl Facts {
    /// The facts of `held` about `delta`.
    fn of(held: &TableSegment, delta: &TableDelta) -> Self {
        let named = delta.touched_rows();
        let incident: BTreeSet<RowId> = delta
            .del_rows()
            .keys()
            .chain(delta.tables().keys())
            .copied()
            .filter(|row| held.rows.contains_key(row))
            .collect();
        let rows: BTreeMap<RowId, (Name, Row)> = held
            .rows
            .iter()
            .filter(|(id, _)| named.contains(id))
            .map(|(id, payload)| (*id, payload.clone()))
            .collect();
        let pairs: BTreeSet<Pair> = held
            .pairs
            .iter()
            .filter(|pair| {
                delta.del_pairs().contains(*pair)
                    || delta.add_pairs().contains(*pair)
                    || incident.contains(&pair.1)
                    || incident.contains(&pair.2)
            })
            .cloned()
            .collect();
        let stubs: BTreeSet<Pair> = held
            .boundary
            .in_stubs
            .keys()
            .filter_map(|elem| match elem {
                ViewRef::Pair { edge, src, tgt } if incident.contains(tgt) => {
                    Some((edge.clone(), *src, *tgt))
                }
                _ => None,
            })
            .collect();
        let mut tables = BTreeMap::new();
        for (_, src, tgt) in pairs.iter().chain(&stubs) {
            for end in [*src, *tgt] {
                if !rows.contains_key(&end)
                    && let Some(table) = held_table(held, end)
                {
                    tables.insert(end, table.clone());
                }
            }
        }
        Self {
            rows,
            pairs,
            stubs,
            tables,
        }
    }

    /// The facts' canonical bytes: the rows as row records, the pairs and
    /// then the stubs as pair records, then the tables as row id and name,
    /// each part counted and ascending.
    fn bytes(&self) -> Vec<u8> {
        let mut sink = CanonicalWriter::new();
        sink.len(self.rows.len());
        for (id, (table, row)) in &self.rows {
            write_row_record(&mut sink, *id, table, row);
        }
        for part in [&self.pairs, &self.stubs] {
            sink.len(part.len());
            for (edge, src, tgt) in part {
                write_pair_record(&mut sink, edge, *src, *tgt);
            }
        }
        sink.len(self.tables.len());
        for (id, table) in &self.tables {
            sink.u32(id.0);
            sink.name(table);
        }
        sink.into_bytes()
    }

    fn decode(bytes: &[u8]) -> Result<Self, SegmentError> {
        let mut r = Reader::new(bytes, ArtifactKind::Summary);
        let n = r.count(20)?;
        let mut rows = BTreeMap::new();
        let mut last = None;
        for _ in 0..n {
            let (id, table, row) = r.row_record()?;
            ascending(&r, &mut last, &id)?;
            rows.insert(id, (table, row));
        }
        let mut parts = [BTreeSet::new(), BTreeSet::new()];
        for part in &mut parts {
            let n = r.count(33)?;
            let mut last = None;
            for _ in 0..n {
                let pair = r.pair_record()?;
                ascending(&r, &mut last, &pair)?;
                part.insert(pair);
            }
        }
        let n = r.count(12)?;
        let mut tables = BTreeMap::new();
        let mut last = None;
        for _ in 0..n {
            let id = RowId(r.u32()?);
            ascending(&r, &mut last, &id)?;
            tables.insert(id, r.name()?);
        }
        r.finish()?;
        let [pairs, stubs] = parts;
        Ok(Self {
            rows,
            pairs,
            stubs,
            tables,
        })
    }
}

/// Every segment's facts, decoded.
fn decode_all(facts: &[Vec<u8>]) -> Result<Vec<Facts>, SegmentError> {
    facts.iter().map(|bytes| Facts::decode(bytes)).collect()
}

/// The table the facts of the segments a delta reads describe: every
/// reported row, every reported pair and stub, and a row without cells in
/// its reported table for every endpoint no fact reports in full.
fn facts_table(facts: &[Facts]) -> Result<FInstance, EditError> {
    let mut rows: BTreeMap<RowId, (Name, Row)> = BTreeMap::new();
    let mut pairs: BTreeSet<Pair> = BTreeSet::new();
    let mut tables: BTreeMap<RowId, Name> = BTreeMap::new();
    for fact in facts {
        rows.extend(fact.rows.iter().map(|(id, row)| (*id, row.clone())));
        pairs.extend(fact.pairs.iter().chain(&fact.stubs).cloned());
        tables.extend(fact.tables.iter().map(|(id, table)| (*id, table.clone())));
    }
    for (id, table) in tables {
        rows.entry(id).or_insert_with(|| (table, Row::new()));
    }
    let mut by_table: BTreeMap<Name, Vec<(RowId, Row)>> = BTreeMap::new();
    for (id, (table, row)) in rows {
        by_table.entry(table).or_default().push((id, row));
    }
    let mut by_edge: BTreeMap<Edge, Vec<(RowId, RowId)>> = BTreeMap::new();
    for (edge, src, tgt) in pairs {
        by_edge.entry(edge).or_default().push((src, tgt));
    }
    FInstance::from_parts(by_table, by_edge)
        .map_err(|error| EditError::ApplyFailed(error.to_string()))
}

/// The table of every row the facts report, in full or by its table, as
/// it is after `delta`.
fn anchors_after(delta: &TableDelta, facts: &[Facts]) -> BTreeMap<RowId, Name> {
    let mut anchors: BTreeMap<RowId, Name> = BTreeMap::new();
    for fact in facts {
        anchors.extend(fact.tables.iter().map(|(id, table)| (*id, table.clone())));
        anchors.extend(
            fact.rows
                .iter()
                .map(|(id, (table, _))| (*id, table.clone())),
        );
    }
    for (id, change) in delta.tables() {
        anchors.insert(*id, change.new.clone());
    }
    anchors.extend(
        delta
            .add_rows()
            .iter()
            .map(|(id, (table, _))| (*id, table.clone())),
    );
    anchors
}

/// The segment `held` after `delta`, once the delta is checked: its own
/// rows and pairs as the delta deletes, adds and changes them, a ghost for
/// every remote target of its pairs and an in-stub for every remote pair
/// entering it, with the tables after the delta (`anchors`, then the
/// segment's own copies for the rows the delta leaves alone).
fn edit_segment(
    held: &TableSegment,
    delta: &TableDelta,
    family: &FamilyIndex,
    anchors: &BTreeMap<RowId, Name>,
) -> Result<TableSegment, SegmentError> {
    let here = held.segment;
    let is_here = |row: RowId| family.owner(row.0) == Some(here);
    let mut rows = held.rows.clone();
    for id in delta.del_rows().keys() {
        rows.remove(id);
    }
    for (id, payload) in delta.add_rows() {
        if is_here(*id) {
            rows.insert(*id, payload.clone());
        }
    }
    for (id, change) in delta.tables() {
        if let Some((table, _)) = rows.get_mut(id) {
            table.clone_from(&change.new);
        }
    }
    for ((id, column), change) in delta.cells() {
        if let Some((_, cells)) = rows.get_mut(id) {
            match &change.new {
                Some(value) => {
                    cells.insert(column.clone(), value.clone());
                }
                None => {
                    cells.remove(column);
                }
            }
        }
    }
    let mut pairs = held.pairs.clone();
    for pair in delta.del_pairs() {
        pairs.remove(pair);
    }
    for pair in delta.add_pairs() {
        if is_here(pair.1) {
            pairs.insert(pair.clone());
        }
    }
    let anchor = |row: RowId| {
        anchors
            .get(&row)
            .or_else(|| held_table(held, row))
            .cloned()
            .unwrap_or_default()
    };
    let owner_or_refuse = |row: RowId, pair: &Pair| {
        family.owner(row.0).ok_or_else(|| SegmentError::Unowned {
            elem: ViewRef::Pair {
                edge: pair.0.clone(),
                src: pair.1,
                tgt: pair.2,
            },
        })
    };
    let mut boundary = Boundary::default();
    for pair in &pairs {
        let tgt = pair.2;
        let owner = owner_or_refuse(tgt, pair)?;
        if owner != here {
            boundary.ghosts.entry(tgt.0).or_insert_with(|| Ghost {
                node: tgt.0,
                owner,
                anchor: anchor(tgt),
            });
        }
    }
    for (elem, stub) in &held.boundary.in_stubs {
        let ViewRef::Pair { edge, src, tgt } = elem else {
            return Err(SegmentError::InStubMismatch { elem: elem.clone() });
        };
        if delta.del_pairs().contains(&(edge.clone(), *src, *tgt)) {
            continue;
        }
        let mut stub = stub.clone();
        stub.src_anchor = anchor(*src);
        boundary.in_stubs.insert(elem.clone(), stub);
    }
    for pair in delta.add_pairs() {
        let (edge, src, tgt) = pair;
        let src_owner = owner_or_refuse(*src, pair)?;
        if is_here(*tgt) && src_owner != here {
            boundary.in_stubs.insert(
                ViewRef::Pair {
                    edge: edge.clone(),
                    src: *src,
                    tgt: *tgt,
                },
                InStub {
                    src: src.0,
                    src_owner,
                    src_anchor: anchor(*src),
                    tgt: tgt.0,
                    label: edge.clone(),
                },
            );
        }
    }
    Ok(TableSegment {
        segment: here,
        rows,
        pairs,
        boundary,
    })
}
