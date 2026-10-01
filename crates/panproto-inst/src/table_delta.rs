//! The canonical delta of a table instance.
//!
//! A [`TableDelta`] mirrors a [`GraphDelta`](crate::graph_edit::GraphDelta)
//! through the graph form of a table
//! ([`GInstance::from_finstance`](crate::GInstance::from_finstance)): rows are
//! nodes, foreign-key pairs are edges, a row's cells are a node's fields and a
//! row's table is a node's anchor. It reads as a span `S <- K -> S'` at the
//! identity match on row ids and pairs: `del_rows` and `del_pairs` are
//! `S \ K`, `add_rows` and `add_pairs` are `S' \ K`, and `cells` and `tables`
//! change entries of rows kept in `K`. A pair carries no payload beyond its
//! own name `(edge, src, tgt)`.
//!
//! [`TableDelta::apply`] is double-pushout rewriting at that match: it checks
//! the delta once against the instance and then applies it in full, leaving
//! the instance unchanged on any error. The algebra
//! ([`TableDelta::invert`], [`TableDelta::compose`], [`TableDelta::identity`])
//! follows the case table of the graph delta with pairs as payload-free
//! elements and cells and tables as attribution entries;
//! [`TableDelta::independence`] and [`TableDelta::merge`] report and combine
//! deltas of one state; [`TableDelta::diff`] computes the delta between two
//! instances. The canonical form is what makes comparisons of translated
//! deltas exact (Horn et al. 2018, Lemma 4.14, proved for minimal deltas over
//! tuple sets; its transfer to deltas keyed by row id is inferred).

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::{CanonicalWriter, Edge};
use serde::{Deserialize, Serialize};

use crate::edit_error::EditError;
use crate::functor::{FInstance, Row, same_row, same_value, write_pair_record, write_row_record};
use crate::graph_edit::{AttrKey, ComposeError, EditConflict, ElemRef};
use crate::identity::{CanonicalSink, Digest, TABLE_DELTA_CONTEXT};
use crate::value::Value;
use crate::view_ref::RowId;

/// A foreign-key pair: the foreign-key edge, the source row and the target
/// row.
pub type Pair = (Edge, RowId, RowId);

/// A change of one cell of a kept row: `old: None` adds the cell, `new: None`
/// removes it.
///
/// Serialized as a struct with the fields `old` and `new`, both always
/// written and both required on decode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellChange {
    /// The cell before the change; `None` when absent.
    #[serde(deserialize_with = "Option::deserialize")]
    pub old: Option<Value>,
    /// The cell after the change; `None` when absent.
    #[serde(deserialize_with = "Option::deserialize")]
    pub new: Option<Value>,
}

/// A change of the table a kept row belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableChange {
    /// The row's table before the change.
    pub old: Name,
    /// The row's table after the change.
    pub new: Name,
}

/// Whether two optional cells are equal by canonical bytes.
pub(crate) fn same_cell(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => same_value(a, b),
        _ => false,
    }
}

/// Whether two row payloads (table and cells) are equal by canonical bytes.
pub(crate) fn same_payload(a: &(Name, Row), b: &(Name, Row)) -> bool {
    a.0 == b.0 && same_row(&a.1, &b.1)
}

/// Serde of a row map whose cells are written in ascending column order.
mod row_map {
    use std::collections::{BTreeMap, HashMap};

    use panproto_gat::Name;
    use serde::ser::SerializeMap;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::functor::Row;
    use crate::value::Value;
    use crate::view_ref::RowId;

    /// A row's cells, written in ascending column order.
    struct Sorted<'a>(&'a Row);

    impl Serialize for Sorted<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let sorted: BTreeMap<&String, &Value> = self.0.iter().collect();
            sorted.serialize(serializer)
        }
    }

    pub fn serialize<S: Serializer>(
        rows: &BTreeMap<RowId, (Name, Row)>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(rows.len()))?;
        for (id, (table, row)) in rows {
            map.serialize_entry(id, &(table, Sorted(row)))?;
        }
        map.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<RowId, (Name, Row)>, D::Error> {
        let decoded: BTreeMap<RowId, (Name, BTreeMap<String, Value>)> =
            BTreeMap::deserialize(deserializer)?;
        Ok(decoded
            .into_iter()
            .map(|(id, (table, cells))| (id, (table, cells.into_iter().collect::<HashMap<_, _>>())))
            .collect())
    }
}

/// The parts of a [`TableDelta`], in any form: the input of
/// [`TableDelta::normalize`] and the output of [`TableDelta::into_parts`].
///
/// Serialized exactly as a [`TableDelta`] is.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableDeltaParts {
    /// Deleted rows, with the table and cells they have before the delta.
    #[serde(with = "row_map")]
    pub del_rows: BTreeMap<RowId, (Name, Row)>,
    /// Added rows, with their table and cells.
    #[serde(with = "row_map")]
    pub add_rows: BTreeMap<RowId, (Name, Row)>,
    /// Deleted foreign-key pairs.
    pub del_pairs: BTreeSet<Pair>,
    /// Added foreign-key pairs.
    pub add_pairs: BTreeSet<Pair>,
    /// Cell changes of kept rows, by row and column.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub cells: BTreeMap<(RowId, String), CellChange>,
    /// Table changes of kept rows.
    pub tables: BTreeMap<RowId, TableChange>,
}

/// A table delta in canonical form.
///
/// The fields, in this order and serialized under these names, are
/// `del_rows: BTreeMap<RowId, (Name, Row)>`, `add_rows: BTreeMap<RowId,
/// (Name, Row)>`, `del_pairs: BTreeSet<(Edge, RowId, RowId)>`, `add_pairs:
/// BTreeSet<(Edge, RowId, RowId)>`, `cells: BTreeMap<(RowId, String),
/// CellChange>` (written as a sequence of pairs, since its keys are not
/// strings) and `tables: BTreeMap<RowId, TableChange>`; a row's cells are
/// written in ascending column order. The fields are private, so every value
/// is in normal form:
///
/// 1. no cell or table change has `old` equal to `new`;
/// 2. no cell or table change names a row the same delta deletes or adds;
/// 3. a row or pair in both a delete part and an add part is a replacement
///    and is kept even when the two payloads are equal.
///
/// Every constructor, decoding included, goes through
/// [`TableDelta::normalize`]. Equality is equality of the canonical bytes
/// ([`canonical_table_delta_bytes`]), so a `NaN` cell equals itself and
/// `0.0` differs from `-0.0`.
///
/// ```
/// use std::collections::HashMap;
///
/// use panproto_inst::table_delta::TableDelta;
/// use panproto_inst::{FInstance, RowId, Value};
///
/// let mut tables = FInstance::new()
///     .with_rows("user", [(RowId(0), HashMap::from([("name".to_owned(), Value::Str("ada".into()))]))])
///     .unwrap();
/// let delta = TableDelta::identity().with_cell_change(
///     RowId(0),
///     "name",
///     Some(Value::Str("ada".into())),
///     Some(Value::Str("grace".into())),
/// );
/// delta.apply(&mut tables).unwrap();
/// assert_eq!(tables.row(RowId(0)).unwrap()["name"], Value::Str("grace".into()));
/// delta.invert().apply(&mut tables).unwrap();
/// assert_eq!(tables.row(RowId(0)).unwrap()["name"], Value::Str("ada".into()));
/// ```
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(from = "TableDeltaParts")]
pub struct TableDelta {
    #[serde(with = "row_map")]
    del_rows: BTreeMap<RowId, (Name, Row)>,
    #[serde(with = "row_map")]
    add_rows: BTreeMap<RowId, (Name, Row)>,
    del_pairs: BTreeSet<Pair>,
    add_pairs: BTreeSet<Pair>,
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    cells: BTreeMap<(RowId, String), CellChange>,
    tables: BTreeMap<RowId, TableChange>,
}

impl PartialEq for TableDelta {
    fn eq(&self, other: &Self) -> bool {
        table_delta_bytes(self) == table_delta_bytes(other)
    }
}

impl Eq for TableDelta {}

impl From<TableDeltaParts> for TableDelta {
    fn from(parts: TableDeltaParts) -> Self {
        Self::normalize(parts)
    }
}

impl From<TableDelta> for TableDeltaParts {
    fn from(delta: TableDelta) -> Self {
        delta.into_parts()
    }
}

impl TableDelta {
    /// The empty delta, the unit of [`Self::compose`].
    #[must_use]
    pub fn identity() -> Self {
        Self::default()
    }

    /// Bring `parts` into normal form: drop every cell or table change whose
    /// `old` equals its `new` (cells by canonical bytes) and every cell or
    /// table change of a row the parts also delete or add; keep every
    /// replacement.
    #[must_use]
    pub fn normalize(parts: TableDeltaParts) -> Self {
        let TableDeltaParts {
            del_rows,
            add_rows,
            del_pairs,
            add_pairs,
            mut cells,
            mut tables,
        } = parts;
        let structural = |row: &RowId| del_rows.contains_key(row) || add_rows.contains_key(row);
        cells.retain(|(row, _), change| {
            !structural(row) && !same_cell(change.old.as_ref(), change.new.as_ref())
        });
        tables.retain(|row, change| !structural(row) && change.old != change.new);
        Self {
            del_rows,
            add_rows,
            del_pairs,
            add_pairs,
            cells,
            tables,
        }
    }

    /// The parts of this delta.
    #[must_use]
    pub fn into_parts(self) -> TableDeltaParts {
        TableDeltaParts {
            del_rows: self.del_rows,
            add_rows: self.add_rows,
            del_pairs: self.del_pairs,
            add_pairs: self.add_pairs,
            cells: self.cells,
            tables: self.tables,
        }
    }

    /// A copy of this delta's parts.
    #[must_use]
    pub fn to_parts(&self) -> TableDeltaParts {
        self.clone().into_parts()
    }

    /// Deleted rows, by id, with their table and cells before the delta.
    #[must_use]
    pub const fn del_rows(&self) -> &BTreeMap<RowId, (Name, Row)> {
        &self.del_rows
    }

    /// Added rows, by id, with their table and cells.
    #[must_use]
    pub const fn add_rows(&self) -> &BTreeMap<RowId, (Name, Row)> {
        &self.add_rows
    }

    /// Deleted foreign-key pairs.
    #[must_use]
    pub const fn del_pairs(&self) -> &BTreeSet<Pair> {
        &self.del_pairs
    }

    /// Added foreign-key pairs.
    #[must_use]
    pub const fn add_pairs(&self) -> &BTreeSet<Pair> {
        &self.add_pairs
    }

    /// Cell changes of kept rows, by row and column.
    #[must_use]
    pub const fn cells(&self) -> &BTreeMap<(RowId, String), CellChange> {
        &self.cells
    }

    /// Table changes of kept rows.
    #[must_use]
    pub const fn tables(&self) -> &BTreeMap<RowId, TableChange> {
        &self.tables
    }

    /// Whether this is the identity delta.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.del_rows.is_empty()
            && self.add_rows.is_empty()
            && self.del_pairs.is_empty()
            && self.add_pairs.is_empty()
            && self.cells.is_empty()
            && self.tables.is_empty()
    }

    /// This delta with row `id` deleted as well, with the table and cells it
    /// has before the delta. Drops any cell or table change of the row.
    #[must_use]
    pub fn with_deleted_row(self, id: RowId, table: impl Into<Name>, row: Row) -> Self {
        let mut parts = self.into_parts();
        parts.del_rows.insert(id, (table.into(), row));
        Self::normalize(parts)
    }

    /// This delta with row `id` added as well. Drops any cell or table
    /// change of the row.
    #[must_use]
    pub fn with_added_row(self, id: RowId, table: impl Into<Name>, row: Row) -> Self {
        let mut parts = self.into_parts();
        parts.add_rows.insert(id, (table.into(), row));
        Self::normalize(parts)
    }

    /// This delta with the pair `(edge, src, tgt)` deleted as well.
    #[must_use]
    pub fn with_deleted_pair(mut self, edge: Edge, src: RowId, tgt: RowId) -> Self {
        self.del_pairs.insert((edge, src, tgt));
        self
    }

    /// This delta with the pair `(edge, src, tgt)` added as well.
    #[must_use]
    pub fn with_added_pair(mut self, edge: Edge, src: RowId, tgt: RowId) -> Self {
        self.add_pairs.insert((edge, src, tgt));
        self
    }

    /// This delta with the cell change `(row, column): old -> new` as well,
    /// replacing any change of that cell it has.
    #[must_use]
    pub fn with_cell_change(
        self,
        row: RowId,
        column: impl Into<String>,
        old: Option<Value>,
        new: Option<Value>,
    ) -> Self {
        let mut parts = self.into_parts();
        parts
            .cells
            .insert((row, column.into()), CellChange { old, new });
        Self::normalize(parts)
    }

    /// This delta with the table change `row: old -> new` as well,
    /// replacing any table change of the row it has.
    #[must_use]
    pub fn with_table_change(self, row: RowId, old: impl Into<Name>, new: impl Into<Name>) -> Self {
        let mut parts = self.into_parts();
        parts.tables.insert(
            row,
            TableChange {
                old: old.into(),
                new: new.into(),
            },
        );
        Self::normalize(parts)
    }

    /// Every row id the delta mentions: deleted, added and changed rows, and
    /// the endpoints of deleted and added pairs.
    #[must_use]
    pub fn touched_rows(&self) -> BTreeSet<RowId> {
        let mut ids: BTreeSet<RowId> = self
            .del_rows
            .keys()
            .chain(self.add_rows.keys())
            .chain(self.tables.keys())
            .copied()
            .collect();
        ids.extend(self.cells.keys().map(|(row, _)| *row));
        for (_, src, tgt) in self.del_pairs.iter().chain(&self.add_pairs) {
            ids.insert(*src);
            ids.insert(*tgt);
        }
        ids
    }

    /// Every pair the delta deletes or adds.
    #[must_use]
    pub fn touched_pairs(&self) -> BTreeSet<Pair> {
        self.del_pairs.union(&self.add_pairs).cloned().collect()
    }
}

/// The first column, in ascending order, in which two rows differ by
/// canonical bytes.
fn first_differing_column(a: &Row, b: &Row) -> Option<String> {
    let columns: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    columns
        .into_iter()
        .find(|column| !same_cell(a.get(*column), b.get(*column)))
        .cloned()
}

/// An instance error raised by a mutation the checks have already cleared,
/// reported rather than unwrapped.
fn internal(err: &crate::error::InstError) -> EditError {
    EditError::ApplyFailed(err.to_string())
}

impl TableDelta {
    /// Check that this delta applies to `instance`, without changing it.
    ///
    /// The checks, in this order, each over its part in ascending key order:
    ///
    /// 1. each deleted row exists ([`EditError::RowNotFound`]), is in the
    ///    table the delta expects ([`EditError::StaleTable`]) and holds the
    ///    cells it expects ([`EditError::StaleCell`], naming the least
    ///    column that differs);
    /// 2. each deleted pair exists ([`EditError::ForeignKeyNotFound`]);
    /// 3. each cell change names an existing row
    ///    ([`EditError::RowNotFound`]) whose cell equals the change's `old`
    ///    ([`EditError::StaleCell`]), then each table change names an
    ///    existing row whose table equals the change's `old`
    ///    ([`EditError::StaleTable`]);
    /// 4. the dangling condition: every pair incident to a deleted row is
    ///    deleted too ([`EditError::DanglingForeignKeys`], listing the
    ///    others);
    /// 5. after the deletions, every added row id is free
    ///    ([`EditError::DuplicateRow`]) and every added pair is absent
    ///    ([`EditError::DuplicateForeignKey`]);
    /// 6. every pair of the result joins its edge's endpoint tables: each
    ///    added pair, then each kept pair incident to a row whose table
    ///    changes, names rows that the result holds in the edge's source
    ///    and target tables ([`EditError::MissingRowEndpoint`], naming the
    ///    first row, source before target, that it does not).
    ///
    /// Cells and rows compare by canonical bytes. `check` succeeds exactly
    /// when [`Self::apply`] does, with the same error.
    ///
    /// # Errors
    ///
    /// The first failing check's error.
    pub fn check(&self, instance: &FInstance) -> Result<(), EditError> {
        for (id, (table, row)) in &self.del_rows {
            let current = instance.table_of(*id).ok_or(EditError::RowNotFound(*id))?;
            if current != table {
                return Err(EditError::StaleTable { row: *id });
            }
            let cells = instance.row(*id).ok_or(EditError::RowNotFound(*id))?;
            if let Some(column) = first_differing_column(cells, row) {
                return Err(EditError::StaleCell { row: *id, column });
            }
        }
        for (edge, src, tgt) in &self.del_pairs {
            if !instance.contains_pair(edge, *src, *tgt) {
                return Err(EditError::ForeignKeyNotFound {
                    edge: edge.clone(),
                    src: *src,
                    tgt: *tgt,
                });
            }
        }
        for ((id, column), change) in &self.cells {
            let cells = instance.row(*id).ok_or(EditError::RowNotFound(*id))?;
            if !same_cell(cells.get(column), change.old.as_ref()) {
                return Err(EditError::StaleCell {
                    row: *id,
                    column: column.clone(),
                });
            }
        }
        for (id, change) in &self.tables {
            let current = instance.table_of(*id).ok_or(EditError::RowNotFound(*id))?;
            if *current != change.old {
                return Err(EditError::StaleTable { row: *id });
            }
        }
        for id in self.del_rows.keys() {
            let kept: Vec<Pair> = instance
                .incident_pairs(*id)
                .into_iter()
                .filter(|pair| !self.del_pairs.contains(pair))
                .collect();
            if !kept.is_empty() {
                return Err(EditError::DanglingForeignKeys {
                    row: *id,
                    pairs: kept,
                });
            }
        }
        for id in self.add_rows.keys() {
            if instance.table_of(*id).is_some() && !self.del_rows.contains_key(id) {
                return Err(EditError::DuplicateRow(*id));
            }
        }
        for pair @ (edge, src, tgt) in &self.add_pairs {
            if instance.contains_pair(edge, *src, *tgt) && !self.del_pairs.contains(pair) {
                return Err(EditError::DuplicateForeignKey {
                    edge: edge.clone(),
                    src: *src,
                    tgt: *tgt,
                });
            }
        }
        for pair in &self.add_pairs {
            self.check_joins(instance, pair)?;
        }
        for id in self.tables.keys() {
            for pair in instance.incident_pairs(*id) {
                if !self.del_pairs.contains(&pair) {
                    self.check_joins(instance, &pair)?;
                }
            }
        }
        Ok(())
    }

    /// The table row `id` is in after the delta, `None` when it has none.
    fn table_after<'a>(&'a self, instance: &'a FInstance, id: RowId) -> Option<&'a Name> {
        if let Some((table, _)) = self.add_rows.get(&id) {
            return Some(table);
        }
        if self.del_rows.contains_key(&id) {
            return None;
        }
        self.tables
            .get(&id)
            .map(|change| &change.new)
            .or_else(|| instance.table_of(id))
    }

    /// Check 6 of [`Self::check`] for one pair of the result.
    fn check_joins(&self, instance: &FInstance, (edge, src, tgt): &Pair) -> Result<(), EditError> {
        for (row, table) in [(*src, &edge.src), (*tgt, &edge.tgt)] {
            if self.table_after(instance, row) != Some(table) {
                return Err(EditError::MissingRowEndpoint {
                    edge: edge.clone(),
                    row,
                });
            }
        }
        Ok(())
    }

    /// Apply this delta to `instance`: delete pairs, then rows, then add
    /// rows, move rows, change cells and add pairs, after checking the whole
    /// delta against the input ([`Self::check`] lists the checks). On any
    /// error `instance` is left unchanged, and the instance is never copied.
    ///
    /// # Errors
    ///
    /// The first failing check's error, as [`Self::check`] reports it.
    pub fn apply(&self, instance: &mut FInstance) -> Result<(), EditError> {
        self.check(instance)?;
        for (edge, src, tgt) in &self.del_pairs {
            instance.remove_foreign_key(edge, *src, *tgt);
        }
        for id in self.del_rows.keys() {
            instance.remove_row(*id);
        }
        for (id, (table, row)) in &self.add_rows {
            instance
                .insert_row(table.clone(), *id, row.clone())
                .map_err(|e| internal(&e))?;
        }
        for (id, change) in &self.tables {
            instance
                .move_row(*id, change.new.clone())
                .map_err(|e| internal(&e))?;
        }
        for ((id, column), change) in &self.cells {
            if let Some(cells) = instance.row_mut(*id) {
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
        for (edge, src, tgt) in &self.add_pairs {
            instance
                .insert_foreign_key(edge.clone(), *src, *tgt)
                .map_err(|e| internal(&e))?;
        }
        Ok(())
    }
}

/// One attribution entry of a row: a cell or the row's table.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum RowKey {
    Cell(String),
    Table,
}

/// The value of one attribution entry of a row.
#[derive(Clone, Debug)]
enum RowVal {
    Cell(Option<Value>),
    Table(Name),
}

impl RowKey {
    /// The entry's [`AttrKey`] on row `row`.
    fn attr(&self, row: RowId) -> AttrKey {
        match self {
            Self::Cell(column) => AttrKey::Cell {
                row,
                column: column.clone(),
            },
            Self::Table => AttrKey::Table { row },
        }
    }

    /// The entry of a row payload.
    fn get(&self, payload: &(Name, Row)) -> RowVal {
        match self {
            Self::Cell(column) => RowVal::Cell(payload.1.get(column).cloned()),
            Self::Table => RowVal::Table(payload.0.clone()),
        }
    }

    /// Write an entry into a row payload.
    fn set(&self, payload: &mut (Name, Row), value: &RowVal) {
        match (self, value) {
            (Self::Cell(column), RowVal::Cell(cell)) => match cell {
                Some(v) => {
                    payload.1.insert(column.clone(), v.clone());
                }
                None => {
                    payload.1.remove(column);
                }
            },
            (Self::Table, RowVal::Table(table)) => payload.0 = table.clone(),
            _ => {}
        }
    }
}

/// Whether two entry values are equal by canonical bytes.
fn same_val(a: &RowVal, b: &RowVal) -> bool {
    match (a, b) {
        (RowVal::Cell(a), RowVal::Cell(b)) => same_cell(a.as_ref(), b.as_ref()),
        (RowVal::Table(a), RowVal::Table(b)) => a == b,
        _ => false,
    }
}

/// A change of one row entry.
#[derive(Clone, Debug)]
struct RowChange {
    old: RowVal,
    new: RowVal,
}

/// What one delta does to one row.
enum RowAction {
    None,
    Del((Name, Row)),
    Add((Name, Row)),
    Repl((Name, Row), (Name, Row)),
    Attrs(Vec<(RowKey, RowChange)>),
}

/// What one delta does to one pair.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PairAction {
    None,
    Del,
    Add,
    Repl,
}

/// Take a delta's action on row `id` out of its parts.
fn take_row(parts: &mut TableDeltaParts, id: RowId) -> RowAction {
    let del = parts.del_rows.remove(&id);
    let add = parts.add_rows.remove(&id);
    let columns: Vec<String> = parts
        .cells
        .range((id, String::new())..)
        .take_while(|((row, _), _)| *row == id)
        .map(|((_, column), _)| column.clone())
        .collect();
    let mut attrs = Vec::with_capacity(columns.len() + 1);
    for column in columns {
        if let Some(change) = parts.cells.remove(&(id, column.clone())) {
            attrs.push((
                RowKey::Cell(column),
                RowChange {
                    old: RowVal::Cell(change.old),
                    new: RowVal::Cell(change.new),
                },
            ));
        }
    }
    if let Some(change) = parts.tables.remove(&id) {
        attrs.push((
            RowKey::Table,
            RowChange {
                old: RowVal::Table(change.old),
                new: RowVal::Table(change.new),
            },
        ));
    }
    match (del, add) {
        (Some(p), Some(q)) => RowAction::Repl(p, q),
        (Some(p), None) => RowAction::Del(p),
        (None, Some(q)) => RowAction::Add(q),
        (None, None) if attrs.is_empty() => RowAction::None,
        (None, None) => RowAction::Attrs(attrs),
    }
}

/// Write a row action back into parts.
fn put_row(parts: &mut TableDeltaParts, id: RowId, action: RowAction) {
    match action {
        RowAction::None => {}
        RowAction::Del(p) => {
            parts.del_rows.insert(id, p);
        }
        RowAction::Add(q) => {
            parts.add_rows.insert(id, q);
        }
        RowAction::Repl(p, q) => {
            parts.del_rows.insert(id, p);
            parts.add_rows.insert(id, q);
        }
        RowAction::Attrs(attrs) => {
            for (key, change) in attrs {
                if same_val(&change.old, &change.new) {
                    continue;
                }
                match (key, change.old, change.new) {
                    (RowKey::Cell(column), RowVal::Cell(old), RowVal::Cell(new)) => {
                        parts.cells.insert((id, column), CellChange { old, new });
                    }
                    (RowKey::Table, RowVal::Table(old), RowVal::Table(new)) => {
                        parts.tables.insert(id, TableChange { old, new });
                    }
                    _ => {}
                }
            }
        }
    }
}

/// A delta's action on one row, read without taking it.
fn row_action(delta: &TableDelta, id: RowId) -> RowAction {
    let mut parts = TableDeltaParts::default();
    if let Some(p) = delta.del_rows.get(&id) {
        parts.del_rows.insert(id, p.clone());
    }
    if let Some(q) = delta.add_rows.get(&id) {
        parts.add_rows.insert(id, q.clone());
    }
    for ((row, column), change) in delta.cells.range((id, String::new())..) {
        if *row != id {
            break;
        }
        parts.cells.insert((id, column.clone()), change.clone());
    }
    if let Some(change) = delta.tables.get(&id) {
        parts.tables.insert(id, change.clone());
    }
    take_row(&mut parts, id)
}

/// Apply entry changes to a payload that must show their `old` values (or,
/// with `undo`, undo changes on one that must show their `new` values).
fn fold(
    payload: &(Name, Row),
    id: RowId,
    attrs: &[(RowKey, RowChange)],
    undo: bool,
) -> Result<(Name, Row), ComposeError> {
    let mut out = payload.clone();
    for (key, change) in attrs {
        let (expect, write) = if undo {
            (&change.new, &change.old)
        } else {
            (&change.old, &change.new)
        };
        if !same_val(&key.get(&out), expect) {
            return Err(ComposeError::Mismatch {
                elem: ElemRef::Attr(key.attr(id)),
            });
        }
        key.set(&mut out, write);
    }
    Ok(out)
}

/// Compose two entry change lists of one row.
fn merge_attrs(
    id: RowId,
    first: Vec<(RowKey, RowChange)>,
    next: Vec<(RowKey, RowChange)>,
) -> Result<Vec<(RowKey, RowChange)>, ComposeError> {
    let mut out: BTreeMap<RowKey, RowChange> = first.into_iter().collect();
    for (key, second) in next {
        match out.remove(&key) {
            None => {
                out.insert(key, second);
            }
            Some(change) => {
                if !same_val(&change.new, &second.old) {
                    return Err(ComposeError::Mismatch {
                        elem: ElemRef::Attr(key.attr(id)),
                    });
                }
                out.insert(
                    key,
                    RowChange {
                        old: change.old,
                        new: second.new,
                    },
                );
            }
        }
    }
    Ok(out.into_iter().collect())
}

/// Compose the actions of two deltas on one row, by the case table of
/// [`TableDelta::compose`].
fn compose_row(id: RowId, first: RowAction, next: RowAction) -> Result<RowAction, ComposeError> {
    let mismatch = || ComposeError::Mismatch {
        elem: ElemRef::Row(id),
    };
    Ok(match (first, next) {
        (a, RowAction::None) => a,
        (RowAction::None, b) => b,
        (RowAction::Del(p), RowAction::Add(r)) => RowAction::Repl(p, r),
        (RowAction::Add(q), RowAction::Del(r)) if same_payload(&q, &r) => RowAction::None,
        (RowAction::Add(q), RowAction::Repl(r, t)) if same_payload(&q, &r) => RowAction::Add(t),
        (RowAction::Add(q), RowAction::Attrs(a)) => RowAction::Add(fold(&q, id, &a, false)?),
        (RowAction::Repl(p, q), RowAction::Del(r)) if same_payload(&q, &r) => RowAction::Del(p),
        (RowAction::Repl(p, q), RowAction::Repl(r, t)) if same_payload(&q, &r) => {
            RowAction::Repl(p, t)
        }
        (RowAction::Repl(p, q), RowAction::Attrs(a)) => {
            RowAction::Repl(p, fold(&q, id, &a, false)?)
        }
        (RowAction::Attrs(a), RowAction::Del(r)) => RowAction::Del(fold(&r, id, &a, true)?),
        (RowAction::Attrs(a), RowAction::Repl(r, t)) => RowAction::Repl(fold(&r, id, &a, true)?, t),
        (RowAction::Attrs(a), RowAction::Attrs(b)) => RowAction::Attrs(merge_attrs(id, a, b)?),
        _ => return Err(mismatch()),
    })
}

/// Compose the actions of two deltas on one pair, by the case table of
/// [`TableDelta::compose`].
fn compose_pair(
    pair: &Pair,
    first: PairAction,
    next: PairAction,
) -> Result<PairAction, ComposeError> {
    use PairAction::{Add, Del, None as Keep, Repl};
    Ok(match (first, next) {
        (a, Keep) => a,
        (Keep, b) => b,
        (Del, Add) | (Repl, Repl) => Repl,
        (Add, Del) => Keep,
        (Add, Repl) => Add,
        (Repl, Del) => Del,
        _ => {
            return Err(ComposeError::Mismatch {
                elem: ElemRef::Pair {
                    edge: Box::new(pair.0.clone()),
                    src: pair.1,
                    tgt: pair.2,
                },
            });
        }
    })
}

/// A delta's action on one pair.
fn pair_action(del: &BTreeSet<Pair>, add: &BTreeSet<Pair>, pair: &Pair) -> PairAction {
    match (del.contains(pair), add.contains(pair)) {
        (true, true) => PairAction::Repl,
        (true, false) => PairAction::Del,
        (false, true) => PairAction::Add,
        (false, false) => PairAction::None,
    }
}

/// Compose `next` onto the parts of a delta in place: the parts become the
/// net delta of the two. Only the elements `next` touches are read, so the
/// cost is that of `next`, logarithmic in the parts.
///
/// # Errors
///
/// The first mismatch, in the order rows then pairs, each ascending; the
/// parts are then partly composed and must be discarded.
pub(crate) fn compose_into(
    parts: &mut TableDeltaParts,
    next: &TableDelta,
) -> Result<(), ComposeError> {
    let mut rows: BTreeSet<RowId> = next
        .del_rows
        .keys()
        .chain(next.add_rows.keys())
        .chain(next.tables.keys())
        .copied()
        .collect();
    rows.extend(next.cells.keys().map(|(row, _)| *row));
    for id in rows {
        let first = take_row(parts, id);
        let composite = compose_row(id, first, row_action(next, id))?;
        put_row(parts, id, composite);
    }
    for pair in next.del_pairs.union(&next.add_pairs) {
        let first = pair_action(&parts.del_pairs, &parts.add_pairs, pair);
        let second = pair_action(&next.del_pairs, &next.add_pairs, pair);
        let composite = compose_pair(pair, first, second)?;
        parts.del_pairs.remove(pair);
        parts.add_pairs.remove(pair);
        if matches!(composite, PairAction::Del | PairAction::Repl) {
            parts.del_pairs.insert(pair.clone());
        }
        if matches!(composite, PairAction::Add | PairAction::Repl) {
            parts.add_pairs.insert(pair.clone());
        }
    }
    Ok(())
}

impl TableDelta {
    /// The inverse delta: deletions and additions swap, and every cell and
    /// table change's `old` and `new` swap. `invert(invert(d)) == d`, and
    /// applying a delta and then its inverse restores the instance.
    #[must_use]
    pub fn invert(&self) -> Self {
        let parts = self.to_parts();
        Self {
            del_rows: parts.add_rows,
            add_rows: parts.del_rows,
            del_pairs: parts.add_pairs,
            add_pairs: parts.del_pairs,
            cells: parts
                .cells
                .into_iter()
                .map(|(k, c)| {
                    (
                        k,
                        CellChange {
                            old: c.new,
                            new: c.old,
                        },
                    )
                })
                .collect(),
            tables: parts
                .tables
                .into_iter()
                .map(|(k, c)| {
                    (
                        k,
                        TableChange {
                            old: c.new,
                            new: c.old,
                        },
                    )
                })
                .collect(),
        }
    }

    /// The net delta of `self` followed by `next`.
    ///
    /// Rows and pairs are composed one by one, by the case table of
    /// [`GraphDelta::compose`](crate::graph_edit::GraphDelta::compose):
    /// a row is an element whose payload is its table and cells, and whose
    /// attribution entries are its cells (by column) and its table; a pair
    /// is an element without payload, so two payloads of one pair are always
    /// equal and a pair has no attribution entries. In particular a delete
    /// followed by an add of the same row or pair is a replacement, never the
    /// identity, and an add followed by a delete of the same payload cancels.
    /// Several entry changes of one row fold into a later delete or
    /// replacement payload in key order (cells by column, then the table).
    ///
    /// [`TableDelta::identity`] is a unit on both sides, and composition is
    /// associative on composable triples: when `a.compose(&b)` and
    /// `b.compose(&c)` are both defined, the two bracketings are both
    /// defined and equal, or both undefined. `invert(a.compose(&b)?) ==
    /// invert(b).compose(&invert(a))?`.
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError::Mismatch`] naming the first element, in the
    /// order rows then pairs by ascending id, whose actions do not compose:
    /// [`ElemRef::Row`], [`ElemRef::Pair`], or [`ElemRef::Attr`] with a
    /// [`AttrKey::Cell`] or [`AttrKey::Table`] key for a failing entry.
    pub fn compose(&self, next: &Self) -> Result<Self, ComposeError> {
        let mut parts = self.to_parts();
        compose_into(&mut parts, next)?;
        Ok(Self::normalize(parts))
    }
}

/// The element reference of a pair.
fn pair_ref((edge, src, tgt): &Pair) -> ElemRef {
    ElemRef::Pair {
        edge: Box::new(edge.clone()),
        src: *src,
        tgt: *tgt,
    }
}

/// The ways in which `deleter`'s deletions are used by `user`.
fn delete_use(deleter: &TableDelta, user: &TableDelta, out: &mut BTreeSet<EditConflict>) {
    for id in deleter.del_rows.keys() {
        let gone = ElemRef::Row(*id);
        for pair in &user.add_pairs {
            if pair.1 == *id || pair.2 == *id {
                out.insert(EditConflict::DeleteUse {
                    deleted: gone.clone(),
                    user: pair_ref(pair),
                });
            }
        }
        if user.del_rows.contains_key(id) && user.add_rows.contains_key(id) {
            out.insert(EditConflict::DeleteUse {
                deleted: gone.clone(),
                user: gone.clone(),
            });
        }
        for ((row, column), _) in user.cells.range((*id, String::new())..) {
            if row != id {
                break;
            }
            out.insert(EditConflict::DeleteUse {
                deleted: gone.clone(),
                user: ElemRef::Attr(AttrKey::Cell {
                    row: *id,
                    column: column.clone(),
                }),
            });
        }
        if user.tables.contains_key(id) {
            out.insert(EditConflict::DeleteUse {
                deleted: gone.clone(),
                user: ElemRef::Attr(AttrKey::Table { row: *id }),
            });
        }
    }
    for pair in &deleter.del_pairs {
        if user.del_pairs.contains(pair) && user.add_pairs.contains(pair) {
            out.insert(EditConflict::DeleteUse {
                deleted: pair_ref(pair),
                user: pair_ref(pair),
            });
        }
    }
}

/// The ways in which `mover`'s table changes meet `user`'s added pairs: a
/// pair added at a row whose table changes may no longer join its edge's
/// endpoint tables.
fn anchor_use(mover: &TableDelta, user: &TableDelta, out: &mut BTreeSet<EditConflict>) {
    for id in mover.tables.keys() {
        for pair in &user.add_pairs {
            if pair.1 == *id || pair.2 == *id {
                out.insert(EditConflict::AnchorUse {
                    node: id.0,
                    user: pair_ref(pair),
                });
            }
        }
    }
}

impl TableDelta {
    /// Every conflict between this delta and `other`, two deltas of the same
    /// state, in ascending order. The deltas are independent exactly when
    /// the list is empty; then, when both apply to an instance, applying one
    /// after the other in either order, or applying [`Self::merge`], gives
    /// the same result.
    ///
    /// The conflicts are those of the graph form, element for element:
    /// [`EditConflict::DeleteDelete`] and [`EditConflict::AddAdd`] for a row
    /// or a pair both delete or both add (a replacement does both);
    /// [`EditConflict::UpdateUpdate`] for a cell or table both change;
    /// [`EditConflict::DeleteUse`] when one deletes a row and the other adds
    /// or replaces a pair at it, changes one of its entries or replaces it,
    /// or when one deletes a pair the other replaces; and
    /// [`EditConflict::AnchorUse`], naming the row by its id as the graph
    /// form names its node, when one changes a row's table and the other
    /// adds or replaces a pair at it.
    #[must_use]
    pub fn independence(&self, other: &Self) -> Vec<EditConflict> {
        let mut out = BTreeSet::new();
        for id in self
            .del_rows
            .keys()
            .filter(|id| other.del_rows.contains_key(id))
        {
            out.insert(EditConflict::DeleteDelete {
                elem: ElemRef::Row(*id),
            });
        }
        for pair in self.del_pairs.intersection(&other.del_pairs) {
            out.insert(EditConflict::DeleteDelete {
                elem: pair_ref(pair),
            });
        }
        for id in self
            .add_rows
            .keys()
            .filter(|id| other.add_rows.contains_key(id))
        {
            out.insert(EditConflict::AddAdd {
                elem: ElemRef::Row(*id),
            });
        }
        for pair in self.add_pairs.intersection(&other.add_pairs) {
            out.insert(EditConflict::AddAdd {
                elem: pair_ref(pair),
            });
        }
        for (row, column) in self.cells.keys().filter(|k| other.cells.contains_key(*k)) {
            out.insert(EditConflict::UpdateUpdate {
                key: AttrKey::Cell {
                    row: *row,
                    column: column.clone(),
                },
            });
        }
        for row in self.tables.keys().filter(|k| other.tables.contains_key(*k)) {
            out.insert(EditConflict::UpdateUpdate {
                key: AttrKey::Table { row: *row },
            });
        }
        delete_use(self, other, &mut out);
        delete_use(other, self, &mut out);
        anchor_use(self, other, &mut out);
        anchor_use(other, self, &mut out);
        out.into_iter().collect()
    }

    /// The union of this delta and `other`, two independent deltas of the
    /// same state.
    ///
    /// # Errors
    ///
    /// Returns the first conflict of [`Self::independence`] when the two
    /// deltas are not independent, a benign one
    /// ([`EditConflict::is_benign_table`]) included.
    pub fn merge(&self, other: &Self) -> Result<Self, EditConflict> {
        if let Some(conflict) = self.independence(other).into_iter().next() {
            return Err(conflict);
        }
        let mut parts = self.to_parts();
        let theirs = other.to_parts();
        parts.del_rows.extend(theirs.del_rows);
        parts.add_rows.extend(theirs.add_rows);
        parts.del_pairs.extend(theirs.del_pairs);
        parts.add_pairs.extend(theirs.add_pairs);
        parts.cells.extend(theirs.cells);
        parts.tables.extend(theirs.tables);
        Ok(Self::normalize(parts))
    }

    /// The delta from `a` to `b`, keyed by row id and pair.
    ///
    /// A row only in `a` is deleted and one only in `b` is added; a row in
    /// both keeps its id, and a differing cell or table becomes a cell or
    /// table change. A pair only in `a` is deleted and one only in `b` is
    /// added. No replacement is emitted. `diff(a, a)` is the identity and
    /// `diff(a, b).apply(a)` gives `b`.
    #[must_use]
    pub fn diff(a: &FInstance, b: &FInstance) -> Self {
        let mut parts = TableDeltaParts::default();
        for id in a.row_ids() {
            let (Some(table), Some(row)) = (a.table_of(id), a.row(id)) else {
                continue;
            };
            match (b.table_of(id), b.row(id)) {
                (Some(new_table), Some(new_row)) => {
                    if new_table != table {
                        parts.tables.insert(
                            id,
                            TableChange {
                                old: table.clone(),
                                new: new_table.clone(),
                            },
                        );
                    }
                    let columns: BTreeSet<&String> = row.keys().chain(new_row.keys()).collect();
                    for column in columns {
                        let (old, new) = (row.get(column), new_row.get(column));
                        if !same_cell(old, new) {
                            parts.cells.insert(
                                (id, column.clone()),
                                CellChange {
                                    old: old.cloned(),
                                    new: new.cloned(),
                                },
                            );
                        }
                    }
                }
                _ => {
                    parts.del_rows.insert(id, (table.clone(), row.clone()));
                }
            }
        }
        for id in b.row_ids() {
            if a.table_of(id).is_none()
                && let (Some(table), Some(row)) = (b.table_of(id), b.row(id))
            {
                parts.add_rows.insert(id, (table.clone(), row.clone()));
            }
        }
        let (before, after): (BTreeSet<Pair>, BTreeSet<Pair>) = (
            a.all_pairs().into_iter().collect(),
            b.all_pairs().into_iter().collect(),
        );
        parts.del_pairs = before.difference(&after).cloned().collect();
        parts.add_pairs = after.difference(&before).cloned().collect();
        Self::normalize(parts)
    }
}

impl EditConflict {
    /// Whether a conflict between two table deltas is benign: a
    /// `DeleteDelete` of a row whose two deleted payloads (table and cells)
    /// are equal, or of a pair; an `AddAdd` of a row whose two added
    /// payloads are equal, or of a pair; an `UpdateUpdate` of a cell or a
    /// table whose two `new` values are equal. A pair carries no payload, so
    /// two deletes or two adds of one pair are always benign. `a` and `b`
    /// are the two deltas the conflict was found between, in either order.
    /// It is false for every other conflict, and for a conflict that names a
    /// graph element, which a table delta does not hold.
    #[must_use]
    pub fn is_benign_table(&self, a: &TableDelta, b: &TableDelta) -> bool {
        let both = |mine: Option<&(Name, Row)>, theirs: Option<&(Name, Row)>| {
            mine.zip(theirs)
                .is_some_and(|(one, two)| same_payload(one, two))
        };
        match self {
            Self::DeleteDelete { elem } => match elem {
                ElemRef::Row(row) => both(a.del_rows.get(row), b.del_rows.get(row)),
                ElemRef::Pair { edge, src, tgt } => {
                    let pair = ((**edge).clone(), *src, *tgt);
                    a.del_pairs.contains(&pair) && b.del_pairs.contains(&pair)
                }
                _ => false,
            },
            Self::AddAdd { elem } => match elem {
                ElemRef::Row(row) => both(a.add_rows.get(row), b.add_rows.get(row)),
                ElemRef::Pair { edge, src, tgt } => {
                    let pair = ((**edge).clone(), *src, *tgt);
                    a.add_pairs.contains(&pair) && b.add_pairs.contains(&pair)
                }
                _ => false,
            },
            Self::UpdateUpdate { key } => match key {
                AttrKey::Cell { row, column } => {
                    let key = (*row, column.clone());
                    a.cells
                        .get(&key)
                        .zip(b.cells.get(&key))
                        .is_some_and(|(one, two)| same_cell(one.new.as_ref(), two.new.as_ref()))
                }
                AttrKey::Table { row } => a
                    .tables
                    .get(row)
                    .zip(b.tables.get(row))
                    .is_some_and(|(one, two)| one.new == two.new),
                _ => false,
            },
            Self::DeleteUse { .. } | Self::AnchorUse { .. } | Self::FanUse { .. } => false,
        }
    }
}

/// Write the canonical bytes of a table delta, which is always in normal
/// form, into `sink`.
///
/// ```text
/// header    = del_rows, add_rows, del_pairs, add_pairs, cells, tables   (counts, u64 each)
/// del_rows  = row record                        (ascending RowId)
/// add_rows  = row record                        (ascending RowId)
/// del_pairs = pair record                       (ascending (edge, src, tgt))
/// add_pairs = pair record                       (ascending (edge, src, tgt))
/// cells     = row: u32, column, old, new        (ascending (row, column))
/// tables    = row: u32, old: name, new: name    (ascending row)
/// ```
///
/// Row and pair records are those of the canonical bytes of a table
/// instance ([`write_row_record`], [`write_pair_record`]); an optional cell
/// is a presence byte (`0` absent, `1` present) and then the value through
/// [`CanonicalSink::value`]. No record refers to a position in the byte
/// string, so the digest can be streamed.
///
/// ```
/// use panproto_inst::table_delta::{TableDelta, canonical_table_delta_bytes};
/// use panproto_schema::CanonicalWriter;
///
/// let mut writer = CanonicalWriter::new();
/// canonical_table_delta_bytes(&TableDelta::identity(), &mut writer);
/// // Six zero counts.
/// assert_eq!(writer.into_bytes(), vec![0u8; 48]);
/// ```
pub fn canonical_table_delta_bytes<S: CanonicalSink + ?Sized>(delta: &TableDelta, sink: &mut S) {
    sink.len(delta.del_rows.len());
    sink.len(delta.add_rows.len());
    sink.len(delta.del_pairs.len());
    sink.len(delta.add_pairs.len());
    sink.len(delta.cells.len());
    sink.len(delta.tables.len());
    for (id, (table, row)) in delta.del_rows.iter().chain(&delta.add_rows) {
        write_row_record(sink, *id, table, row);
    }
    for (edge, src, tgt) in delta.del_pairs.iter().chain(&delta.add_pairs) {
        write_pair_record(sink, edge, *src, *tgt);
    }
    for ((row, column), change) in &delta.cells {
        sink.u32(row.0);
        sink.str(column);
        for cell in [&change.old, &change.new] {
            match cell {
                None => sink.bool(false),
                Some(value) => {
                    sink.bool(true);
                    sink.value(value);
                }
            }
        }
    }
    for (row, change) in &delta.tables {
        sink.u32(row.0);
        sink.name(&change.old);
        sink.name(&change.new);
    }
}

/// The canonical bytes of a table delta, in a buffer.
fn table_delta_bytes(delta: &TableDelta) -> Vec<u8> {
    let mut writer = CanonicalWriter::new();
    canonical_table_delta_bytes(delta, &mut writer);
    writer.into_bytes()
}

impl TableDelta {
    /// The digest of this delta: its canonical bytes
    /// ([`canonical_table_delta_bytes`]) under [`TABLE_DELTA_CONTEXT`],
    /// computed by streaming the records into the hasher.
    ///
    /// ```
    /// use panproto_inst::identity::{Digest, TABLE_DELTA_CONTEXT};
    /// use panproto_inst::table_delta::{TableDelta, canonical_table_delta_bytes};
    /// use panproto_schema::CanonicalWriter;
    ///
    /// let delta = TableDelta::identity();
    /// let mut writer = CanonicalWriter::new();
    /// canonical_table_delta_bytes(&delta, &mut writer);
    /// assert_eq!(delta.digest(), Digest::derive(TABLE_DELTA_CONTEXT, writer.as_bytes()));
    /// ```
    #[must_use]
    pub fn digest(&self) -> Digest {
        let mut hasher = Digest::hasher(TABLE_DELTA_CONTEXT);
        canonical_table_delta_bytes(self, &mut hasher);
        hasher.finalize()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::table_edit::TableEdit;

    const TABLES: [&str; 3] = ["user", "post", "tag"];
    const ROWS: u32 = 6;

    fn edge(src: &str, tgt: &str, kind: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: kind.into(),
            name: None,
        }
    }

    /// The foreign keys generated instances draw from: post -> user,
    /// user -> user and tag -> post.
    fn edges() -> [Edge; 3] {
        [
            edge("post", "user", "fk"),
            edge("user", "user", "boss"),
            edge("tag", "post", "on"),
        ]
    }

    fn fk() -> Edge {
        edges()[0].clone()
    }

    fn cells(pairs: &[(&str, Value)]) -> Row {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    fn bytes(instance: &FInstance) -> Vec<u8> {
        crate::functor::canonical_finstance_bytes(instance)
    }

    /// Users 0 (alice) and 1 (bob), post 2 by alice.
    fn sample() -> FInstance {
        FInstance::new()
            .with_rows(
                "user",
                [
                    (RowId(0), cells(&[("name", Value::Str("alice".into()))])),
                    (RowId(1), cells(&[("name", Value::Str("bob".into()))])),
                ],
            )
            .unwrap()
            .with_rows(
                "post",
                [(RowId(2), cells(&[("title", Value::Str("hi".into()))]))],
            )
            .unwrap()
            .with_foreign_key(fk(), [(RowId(2), RowId(0))])
            .unwrap()
    }

    /// Cell values, including the floats whose `==` is not an identity.
    fn arb_cell() -> impl Strategy<Value = Value> {
        prop_oneof![
            (-2_i64..2).prop_map(Value::Int),
            Just(Value::Float(f64::NAN)),
            Just(Value::Float(0.0)),
            Just(Value::Float(-0.0)),
            "[xy]".prop_map(Value::Str),
        ]
    }

    fn arb_row() -> impl Strategy<Value = Row> {
        prop::collection::hash_map("[ab]", arb_cell(), 0..3)
    }

    /// A table instance over row ids `0..ROWS`, each row in one of three
    /// tables or absent, with any pairs of the three foreign keys that join
    /// their endpoint tables.
    fn arb_instance() -> impl Strategy<Value = FInstance> {
        let slots = (ROWS * ROWS) as usize;
        (
            prop::collection::vec(
                prop::option::of((0..TABLES.len(), arb_row())),
                ROWS as usize,
            ),
            prop::collection::vec(any::<bool>(), 3 * slots),
        )
            .prop_map(move |(rows, mask)| {
                let mut instance = FInstance::new();
                for (id, row) in rows.into_iter().enumerate() {
                    if let Some((table, row)) = row {
                        instance
                            .insert_row(TABLES[table], RowId(u32::try_from(id).unwrap()), row)
                            .unwrap();
                    }
                }
                for (k, edge) in edges().into_iter().enumerate() {
                    for s in 0..ROWS {
                        for t in 0..ROWS {
                            let at = k * slots + (s * ROWS + t) as usize;
                            let joins = instance.table_of(RowId(s)) == Some(&edge.src)
                                && instance.table_of(RowId(t)) == Some(&edge.tgt);
                            if mask[at] && joins {
                                instance
                                    .insert_foreign_key(edge.clone(), RowId(s), RowId(t))
                                    .unwrap();
                            }
                        }
                    }
                }
                instance
            })
    }

    fn arb_pair() -> impl Strategy<Value = Pair> {
        (0..3usize, 0..ROWS, 0..ROWS).prop_map(|(k, s, t)| (edges()[k].clone(), RowId(s), RowId(t)))
    }

    /// Parts in any form, normal or not.
    fn arb_parts() -> impl Strategy<Value = TableDeltaParts> {
        let row = || {
            (0..ROWS, 0..TABLES.len(), arb_row())
                .prop_map(|(id, t, row)| (RowId(id), (Name::from(TABLES[t]), row)))
        };
        let cell = (
            0..ROWS,
            "[ab]",
            prop::option::of(arb_cell()),
            prop::option::of(arb_cell()),
        )
            .prop_map(|(id, c, old, new)| ((RowId(id), c), CellChange { old, new }));
        let table = (0..ROWS, 0..TABLES.len(), 0..TABLES.len()).prop_map(|(id, a, b)| {
            (
                RowId(id),
                TableChange {
                    old: TABLES[a].into(),
                    new: TABLES[b].into(),
                },
            )
        });
        (
            prop::collection::vec(row(), 0..3),
            prop::collection::vec(row(), 0..3),
            prop::collection::btree_set(arb_pair(), 0..3),
            prop::collection::btree_set(arb_pair(), 0..3),
            prop::collection::vec(cell, 0..4),
            prop::collection::vec(table, 0..3),
        )
            .prop_map(
                |(del_rows, add_rows, del_pairs, add_pairs, cells, tables)| TableDeltaParts {
                    del_rows: del_rows.into_iter().collect(),
                    add_rows: add_rows.into_iter().collect(),
                    del_pairs,
                    add_pairs,
                    cells: cells.into_iter().collect(),
                    tables: tables.into_iter().collect(),
                },
            )
    }

    /// Whether a delta is in normal form.
    fn is_normal(delta: &TableDelta) -> bool {
        let structural =
            |row: &RowId| delta.del_rows().contains_key(row) || delta.add_rows().contains_key(row);
        delta.cells().iter().all(|((row, _), change)| {
            !structural(row) && !same_cell(change.old.as_ref(), change.new.as_ref())
        }) && delta
            .tables()
            .iter()
            .all(|(row, change)| !structural(row) && change.old != change.new)
    }

    #[test]
    fn table_delta_fields_serialize_in_contract_order() {
        let delta = TableDelta::identity()
            .with_deleted_row(
                RowId(1),
                "user",
                cells(&[("b", Value::Int(2)), ("a", Value::Int(1))]),
            )
            .with_added_row(RowId(5), "post", Row::new())
            .with_deleted_pair(fk(), RowId(2), RowId(1))
            .with_added_pair(fk(), RowId(5), RowId(0))
            .with_cell_change(RowId(0), "name", None, Some(Value::Int(3)))
            .with_table_change(RowId(3), "tag", "post");
        let json = serde_json::to_string(&delta).unwrap();
        let fk_json = r#"{"src":"post","tgt":"user","kind":"fk","name":null}"#;
        assert_eq!(
            json,
            format!(
                concat!(
                    r#"{{"del_rows":{{"1":["user",{{"a":{{"Int":1}},"b":{{"Int":2}}}}]}},"#,
                    r#""add_rows":{{"5":["post",{{}}]}},"#,
                    r#""del_pairs":[[{fk},2,1]],"#,
                    r#""add_pairs":[[{fk},5,0]],"#,
                    r#""cells":[[[0,"name"],{{"old":null,"new":{{"Int":3}}}}]],"#,
                    r#""tables":{{"3":{{"old":"tag","new":"post"}}}}}}"#
                ),
                fk = fk_json
            )
        );
        let back: TableDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(back, delta);
        let packed = rmp_serde::to_vec_named(&delta).unwrap();
        assert_eq!(rmp_serde::from_slice::<TableDelta>(&packed).unwrap(), delta);
        let mut cbor = Vec::new();
        ciborium::into_writer(&delta, &mut cbor).unwrap();
        assert_eq!(
            ciborium::from_reader::<TableDelta, _>(cbor.as_slice()).unwrap(),
            delta
        );

        // A cell change spells out both values; decoding normalizes.
        let missing_old = r#"{"del_rows":{},"add_rows":{},"del_pairs":[],"add_pairs":[],"cells":[[[0,"n"],{"new":null}]],"tables":{}}"#;
        let err = serde_json::from_str::<TableDelta>(missing_old).unwrap_err();
        assert!(err.to_string().contains("missing field `old`"), "{err}");
        let no_op = r#"{"del_rows":{},"add_rows":{},"del_pairs":[],"add_pairs":[],"cells":[[[0,"n"],{"old":{"Int":1},"new":{"Int":1}}]],"tables":{"4":{"old":"a","new":"a"}}}"#;
        assert!(
            serde_json::from_str::<TableDelta>(no_op)
                .unwrap()
                .is_identity()
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn every_constructed_table_delta_is_normal(
            a in arb_parts(),
            b in arb_parts(),
            s in arb_instance(),
            t in arb_instance(),
        ) {
            let (a, b) = (TableDelta::normalize(a), TableDelta::normalize(b));
            prop_assert!(is_normal(&a) && is_normal(&b));
            prop_assert!(is_normal(&a.invert()));
            if let Ok(ab) = a.compose(&b) {
                prop_assert!(is_normal(&ab));
            }
            if let Ok(m) = a.merge(&b) {
                prop_assert!(is_normal(&m));
            }
            prop_assert!(is_normal(&TableDelta::diff(&s, &t)));
            // Decoding goes through the normal form too.
            let json = serde_json::to_string(&a.to_parts()).unwrap();
            prop_assert_eq!(&serde_json::from_str::<TableDelta>(&json).unwrap(), &a);
            // A pair in both parts is a replacement and is kept.
            for pair in a.del_pairs().intersection(a.add_pairs()) {
                prop_assert!(a.touched_pairs().contains(pair));
            }
        }
    }

    /// Apply `delta` to a copy of `instance` and expect `err`, with the copy
    /// unchanged; `check` agrees.
    fn refused(instance: &FInstance, delta: &TableDelta, err: &EditError) {
        let mut state = instance.clone();
        assert_eq!(delta.check(&state).as_ref(), Err(err));
        assert_eq!(delta.apply(&mut state).as_ref(), Err(err));
        assert_eq!(bytes(&state), bytes(instance));
    }

    #[test]
    fn table_apply_reports_row_not_found() {
        let delete = TableDelta::identity().with_deleted_row(RowId(9), "user", Row::new());
        refused(&sample(), &delete, &EditError::RowNotFound(RowId(9)));
        let cell =
            TableDelta::identity().with_cell_change(RowId(9), "name", None, Some(Value::Int(1)));
        refused(&sample(), &cell, &EditError::RowNotFound(RowId(9)));
        let table = TableDelta::identity().with_table_change(RowId(9), "user", "post");
        refused(&sample(), &table, &EditError::RowNotFound(RowId(9)));
        let edit = TableEdit::UpdateCell {
            table: "user".into(),
            id: RowId(9),
            column: "name".into(),
            old: Some(Value::Int(1)),
            new: Some(Value::Int(1)),
        };
        assert_eq!(
            edit.apply(&mut sample()),
            Err(EditError::RowNotFound(RowId(9)))
        );
    }

    #[test]
    fn table_apply_reports_duplicate_row() {
        let add = TableDelta::identity().with_added_row(RowId(1), "user", Row::new());
        refused(&sample(), &add, &EditError::DuplicateRow(RowId(1)));
        // Replacing the row frees its id.
        let replace = add.with_deleted_row(
            RowId(1),
            "user",
            cells(&[("name", Value::Str("bob".into()))]),
        );
        let mut state = sample();
        replace.apply(&mut state).unwrap();
        assert_eq!(state.row(RowId(1)), Some(&Row::new()));
    }

    #[test]
    fn table_apply_reports_stale_cell() {
        let delete = TableDelta::identity().with_deleted_row(
            RowId(1),
            "user",
            cells(&[("name", Value::Str("bob".into())), ("age", Value::Int(3))]),
        );
        let stale = EditError::StaleCell {
            row: RowId(1),
            column: "age".into(),
        };
        refused(&sample(), &delete, &stale);
        let change =
            TableDelta::identity().with_cell_change(RowId(1), "age", Some(Value::Int(3)), None);
        refused(&sample(), &change, &stale);
    }

    #[test]
    fn table_apply_reports_stale_table() {
        let delete = TableDelta::identity().with_deleted_row(
            RowId(1),
            "post",
            cells(&[("name", Value::Str("bob".into()))]),
        );
        refused(&sample(), &delete, &EditError::StaleTable { row: RowId(1) });
        let change = TableDelta::identity().with_table_change(RowId(1), "post", "tag");
        refused(&sample(), &change, &EditError::StaleTable { row: RowId(1) });
    }

    #[test]
    fn table_apply_reports_stale_incidence() {
        // The edit lists no pair at user 0, which has one.
        let missing = TableEdit::DeleteRow {
            table: "user".into(),
            id: RowId(0),
            row: cells(&[("name", Value::Str("alice".into()))]),
            incident_fks: Vec::new(),
        };
        let mut state = sample();
        assert_eq!(
            missing.apply(&mut state),
            Err(EditError::StaleIncidence { row: RowId(0) })
        );
        // It lists a pair at another row, which it would otherwise delete
        // along with the row.
        let mut linked = sample();
        linked.insert_foreign_key(fk(), RowId(2), RowId(1)).unwrap();
        let foreign = TableEdit::DeleteRow {
            table: "user".into(),
            id: RowId(1),
            row: cells(&[("name", Value::Str("bob".into()))]),
            incident_fks: vec![(fk(), RowId(2), RowId(0)), (fk(), RowId(2), RowId(1))],
        };
        let before = bytes(&linked);
        assert_eq!(
            foreign.apply(&mut linked),
            Err(EditError::StaleIncidence { row: RowId(1) })
        );
        assert_eq!(bytes(&linked), before);
        // Inside a batch, the incidence is that of the state the step meets:
        // a pair the batch added before is listed.
        let batch = TableEdit::Batch(vec![
            TableEdit::AddForeignKey {
                edge: fk(),
                src: RowId(2),
                tgt: RowId(1),
            },
            TableEdit::DeleteRow {
                table: "user".into(),
                id: RowId(1),
                row: cells(&[("name", Value::Str("bob".into()))]),
                incident_fks: Vec::new(),
            },
        ]);
        let mut state = sample();
        assert_eq!(
            batch.apply(&mut state),
            Err(EditError::BatchStep {
                index: 1,
                source: Box::new(EditError::StaleIncidence { row: RowId(1) })
            })
        );
        assert_eq!(bytes(&state), bytes(&sample()));
    }

    #[test]
    fn table_apply_reports_foreign_key_not_found() {
        let delete = TableDelta::identity().with_deleted_pair(fk(), RowId(2), RowId(1));
        refused(
            &sample(),
            &delete,
            &EditError::ForeignKeyNotFound {
                edge: fk(),
                src: RowId(2),
                tgt: RowId(1),
            },
        );
    }

    #[test]
    fn table_apply_reports_duplicate_foreign_key() {
        let add = TableDelta::identity().with_added_pair(fk(), RowId(2), RowId(0));
        refused(
            &sample(),
            &add,
            &EditError::DuplicateForeignKey {
                edge: fk(),
                src: RowId(2),
                tgt: RowId(0),
            },
        );
        // A pair in both parts is a replacement and applies.
        let mut state = sample();
        add.with_deleted_pair(fk(), RowId(2), RowId(0))
            .apply(&mut state)
            .unwrap();
        assert_eq!(bytes(&state), bytes(&sample()));
    }

    #[test]
    fn table_apply_reports_missing_row_endpoint() {
        // Row 7 does not exist.
        let dangling = TableDelta::identity().with_added_pair(fk(), RowId(2), RowId(7));
        refused(
            &sample(),
            &dangling,
            &EditError::MissingRowEndpoint {
                edge: fk(),
                row: RowId(7),
            },
        );
        // Row 1 is a user, not a post.
        let wrong = TableDelta::identity().with_added_pair(fk(), RowId(1), RowId(0));
        refused(
            &sample(),
            &wrong,
            &EditError::MissingRowEndpoint {
                edge: fk(),
                row: RowId(1),
            },
        );
        // Moving user 0 would leave post 2's pair pointing outside `user`.
        let moved = TableDelta::identity().with_table_change(RowId(0), "user", "tag");
        refused(
            &sample(),
            &moved,
            &EditError::MissingRowEndpoint {
                edge: fk(),
                row: RowId(0),
            },
        );
        // A pair to a row the same delta adds joins.
        let mut state = sample();
        TableDelta::identity()
            .with_added_row(RowId(4), "post", Row::new())
            .with_added_pair(fk(), RowId(4), RowId(1))
            .apply(&mut state)
            .unwrap();
        assert!(state.contains_pair(&fk(), RowId(4), RowId(1)));
    }

    #[test]
    fn table_apply_reports_dangling_foreign_keys() {
        let delete = TableDelta::identity().with_deleted_row(
            RowId(0),
            "user",
            cells(&[("name", Value::Str("alice".into()))]),
        );
        refused(
            &sample(),
            &delete,
            &EditError::DanglingForeignKeys {
                row: RowId(0),
                pairs: vec![(fk(), RowId(2), RowId(0))],
            },
        );
        let mut state = sample();
        delete
            .with_deleted_pair(fk(), RowId(2), RowId(0))
            .apply(&mut state)
            .unwrap();
        assert_eq!(state.pair_count(), 0);
        assert_eq!(state.table_of(RowId(0)), None);
    }

    /// Apply a delta to a copy.
    fn applied(delta: &TableDelta, instance: &FInstance) -> Result<FInstance, EditError> {
        let mut state = instance.clone();
        delta.apply(&mut state).map(|()| state)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn table_check_agrees_with_apply(s in arb_instance(), t in arb_instance(), parts in arb_parts()) {
            for delta in [TableDelta::diff(&s, &t), TableDelta::normalize(parts)] {
                let mut state = s.clone();
                let checked = delta.check(&s);
                let result = delta.apply(&mut state);
                prop_assert_eq!(&checked, &result);
                if result.is_err() {
                    prop_assert_eq!(bytes(&state), bytes(&s), "a refused delta changes nothing");
                }
            }
        }

        #[test]
        fn table_differ_laws(s in arb_instance(), t in arb_instance()) {
            let d = TableDelta::diff(&s, &t);
            prop_assert_eq!(bytes(&applied(&d, &s).unwrap()), bytes(&t));
            prop_assert!(TableDelta::diff(&s, &s).is_identity());
            prop_assert!(d.del_rows().keys().all(|r| !d.add_rows().contains_key(r)));
            prop_assert!(d.del_pairs().is_disjoint(d.add_pairs()));
        }

        #[test]
        fn table_delta_algebra_laws(
            s0 in arb_instance(),
            s1 in arb_instance(),
            s2 in arb_instance(),
            s3 in arb_instance(),
            x in arb_parts(),
            y in arb_parts(),
            z in arb_parts(),
        ) {
            // Consecutive deltas: every composite is defined, applies, and
            // equals applying the steps in turn.
            let (a, b, c) = (
                TableDelta::diff(&s0, &s1),
                TableDelta::diff(&s1, &s2),
                TableDelta::diff(&s2, &s3),
            );
            let ab = a.compose(&b).unwrap();
            let bc = b.compose(&c).unwrap();
            prop_assert_eq!(ab.compose(&c).unwrap(), a.compose(&bc).unwrap());
            prop_assert_eq!(bytes(&applied(&ab, &s0).unwrap()), bytes(&s2));
            prop_assert_eq!(ab.invert(), b.invert().compose(&a.invert()).unwrap());
            // Apply then invert restores, and invert is an involution.
            let ab_s = applied(&ab, &s0).unwrap();
            prop_assert_eq!(bytes(&applied(&ab.invert(), &ab_s).unwrap()), bytes(&s0));
            prop_assert_eq!(&ab.invert().invert(), &ab);
            // Unit on both sides.
            let id = TableDelta::identity();
            prop_assert_eq!(&id.compose(&ab).unwrap(), &ab);
            prop_assert_eq!(&ab.compose(&id).unwrap(), &ab);

            // Arbitrary triples: associative wherever both adjacent
            // composites are defined, and anti-homomorphic.
            let (x, y, z) = (TableDelta::normalize(x), TableDelta::normalize(y), TableDelta::normalize(z));
            if let (Ok(xy), Ok(yz)) = (x.compose(&y), y.compose(&z)) {
                match (xy.compose(&z), x.compose(&yz)) {
                    (Ok(l), Ok(r)) => prop_assert_eq!(l, r),
                    (Err(_), Err(_)) => {}
                    (l, r) => prop_assert!(false, "one bracketing only: {:?} / {:?}", l, r),
                }
                prop_assert_eq!(xy.invert(), y.invert().compose(&x.invert()).unwrap());
            }
            prop_assert_eq!(&x.invert().invert(), &x);
        }

        #[test]
        fn independent_table_deltas_commute_and_merge(
            s in arb_instance(),
            t in arb_instance(),
            u in arb_instance(),
            keep_a in prop::collection::vec(any::<bool>(), 64),
            keep_b in prop::collection::vec(any::<bool>(), 64),
        ) {
            // Two deltas of one state: sub-deltas of the diffs to t and u,
            // keeping each element with a coin, then kept only when each
            // applies alone.
            let pick = |d: TableDelta, keep: &[bool]| {
                let parts = d.into_parts();
                let mut coin = keep.iter().cycle();
                let mut flip = || *coin.next().unwrap_or(&true);
                TableDelta::normalize(TableDeltaParts {
                    del_rows: parts.del_rows.into_iter().filter(|_| flip()).collect(),
                    add_rows: parts.add_rows.into_iter().filter(|_| flip()).collect(),
                    del_pairs: parts.del_pairs.into_iter().filter(|_| flip()).collect(),
                    add_pairs: parts.add_pairs.into_iter().filter(|_| flip()).collect(),
                    cells: parts.cells.into_iter().filter(|_| flip()).collect(),
                    tables: parts.tables.into_iter().filter(|_| flip()).collect(),
                })
            };
            let a = pick(TableDelta::diff(&s, &t), &keep_a);
            let b = pick(TableDelta::diff(&s, &u), &keep_b);
            prop_assume!(a.check(&s).is_ok() && b.check(&s).is_ok());
            let conflicts = a.independence(&b);
            prop_assert_eq!(&conflicts, &{
                let mut sorted = conflicts.clone();
                sorted.sort();
                sorted
            });
            let mut flipped = b.independence(&a);
            flipped.sort();
            prop_assert_eq!(&flipped, &conflicts, "independence is symmetric");
            if conflicts.is_empty() {
                let ab = applied(&b, &applied(&a, &s).unwrap()).unwrap();
                let ba = applied(&a, &applied(&b, &s).unwrap()).unwrap();
                let merged = applied(&a.merge(&b).unwrap(), &s).unwrap();
                prop_assert_eq!(bytes(&ab), bytes(&ba));
                prop_assert_eq!(bytes(&ab), bytes(&merged));
            } else {
                prop_assert_eq!(a.merge(&b), Err(conflicts[0].clone()));
            }
        }
    }

    #[test]
    fn table_compose_case_table() {
        let p = cells(&[("a", Value::Int(1))]);
        let q = cells(&[("a", Value::Int(2))]);
        let r = cells(&[("a", Value::Int(3))]);
        let del =
            |row: &Row| TableDelta::identity().with_deleted_row(RowId(0), "user", row.clone());
        let add = |row: &Row| TableDelta::identity().with_added_row(RowId(0), "user", row.clone());
        let repl = |a: &Row, b: &Row| del(a).with_added_row(RowId(0), "user", b.clone());
        let attr = |o: i64, n: i64| {
            TableDelta::identity().with_cell_change(
                RowId(0),
                "a",
                Some(Value::Int(o)),
                Some(Value::Int(n)),
            )
        };
        let row = || ElemRef::Row(RowId(0));
        let cell = || {
            ElemRef::Attr(AttrKey::Cell {
                row: RowId(0),
                column: "a".into(),
            })
        };
        let mismatch = |elem| Err(ComposeError::Mismatch { elem });
        // Del(p) then ...
        assert_eq!(del(&p).compose(&add(&q)), Ok(repl(&p, &q)));
        assert_eq!(del(&p).compose(&add(&p)), Ok(repl(&p, &p)), "never cancels");
        assert_eq!(del(&p).compose(&del(&p)), mismatch(row()));
        assert_eq!(del(&p).compose(&repl(&p, &q)), mismatch(row()));
        assert_eq!(del(&p).compose(&attr(1, 2)), mismatch(row()));
        // Add(q) then ...
        assert_eq!(add(&q).compose(&del(&q)), Ok(TableDelta::identity()));
        assert_eq!(add(&q).compose(&del(&p)), mismatch(row()));
        assert_eq!(add(&q).compose(&add(&q)), mismatch(row()));
        assert_eq!(add(&q).compose(&repl(&q, &r)), Ok(add(&r)));
        assert_eq!(add(&q).compose(&attr(2, 3)), Ok(add(&r)));
        assert_eq!(add(&q).compose(&attr(1, 3)), mismatch(cell()));
        // Repl(p -> q) then ...
        assert_eq!(repl(&p, &q).compose(&del(&q)), Ok(del(&p)));
        assert_eq!(repl(&p, &q).compose(&del(&r)), mismatch(row()));
        assert_eq!(repl(&p, &q).compose(&add(&r)), mismatch(row()));
        assert_eq!(repl(&p, &q).compose(&repl(&q, &r)), Ok(repl(&p, &r)));
        assert_eq!(repl(&p, &q).compose(&attr(2, 3)), Ok(repl(&p, &r)));
        // Attr(o -> n) then ...
        assert_eq!(attr(1, 2).compose(&del(&q)), Ok(del(&p)));
        assert_eq!(attr(1, 2).compose(&del(&r)), mismatch(cell()));
        assert_eq!(attr(1, 2).compose(&add(&q)), mismatch(row()));
        assert_eq!(attr(1, 2).compose(&repl(&q, &r)), Ok(repl(&p, &r)));
        assert_eq!(attr(1, 2).compose(&attr(2, 3)), Ok(attr(1, 3)));
        assert_eq!(attr(1, 2).compose(&attr(2, 1)), Ok(TableDelta::identity()));
        assert_eq!(attr(1, 2).compose(&attr(3, 1)), mismatch(cell()));
        let other = TableDelta::identity().with_table_change(RowId(0), "user", "tag");
        assert_eq!(
            attr(1, 2).compose(&other),
            Ok(attr(1, 2).with_table_change(RowId(0), "user", "tag")),
            "different keys are both kept"
        );

        // Pairs carry no payload.
        let pair = || ElemRef::Pair {
            edge: Box::new(fk()),
            src: RowId(2),
            tgt: RowId(0),
        };
        let pdel = || TableDelta::identity().with_deleted_pair(fk(), RowId(2), RowId(0));
        let padd = || TableDelta::identity().with_added_pair(fk(), RowId(2), RowId(0));
        let prepl = || pdel().with_added_pair(fk(), RowId(2), RowId(0));
        assert_eq!(pdel().compose(&padd()), Ok(prepl()));
        assert_eq!(padd().compose(&pdel()), Ok(TableDelta::identity()));
        assert_eq!(padd().compose(&prepl()), Ok(padd()));
        assert_eq!(prepl().compose(&pdel()), Ok(pdel()));
        assert_eq!(prepl().compose(&prepl()), Ok(prepl()));
        assert_eq!(pdel().compose(&pdel()), mismatch(pair()));
        assert_eq!(pdel().compose(&prepl()), mismatch(pair()));
        assert_eq!(padd().compose(&padd()), mismatch(pair()));
        assert_eq!(prepl().compose(&padd()), mismatch(pair()));
    }

    #[test]
    fn benign_table_conflicts_are_exactly_the_three_equal_payload_cases() {
        let alice = cells(&[("name", Value::Str("alice".into()))]);
        let del = |row: &Row| {
            TableDelta::identity()
                .with_deleted_row(RowId(0), "user", row.clone())
                .with_deleted_pair(fk(), RowId(2), RowId(0))
        };
        let (first, second) = (del(&alice), del(&alice));
        let conflicts = first.independence(&second);
        assert_eq!(
            conflicts,
            vec![
                EditConflict::DeleteDelete {
                    elem: ElemRef::Row(RowId(0))
                },
                EditConflict::DeleteDelete {
                    elem: ElemRef::Pair {
                        edge: Box::new(fk()),
                        src: RowId(2),
                        tgt: RowId(0)
                    }
                },
            ]
        );
        assert!(
            conflicts
                .iter()
                .all(|conflict| conflict.is_benign_table(&first, &second))
        );
        let other = del(&cells(&[("name", Value::Str("alicia".into()))]));
        let mixed = first.independence(&other);
        assert!(
            !mixed[0].is_benign_table(&first, &other),
            "row payloads differ"
        );
        assert!(
            mixed[1].is_benign_table(&first, &other),
            "pairs have no payload"
        );

        let add = |n: i64| {
            TableDelta::identity()
                .with_added_row(RowId(7), "tag", cells(&[("n", Value::Int(n))]))
                .with_added_pair(fk(), RowId(2), RowId(1))
        };
        let (one, two, three) = (add(1), add(1), add(2));
        assert!(
            one.independence(&two)
                .iter()
                .all(|conflict| conflict.is_benign_table(&one, &two))
        );
        let differing = one.independence(&three);
        assert_eq!(
            differing[0],
            EditConflict::AddAdd {
                elem: ElemRef::Row(RowId(7))
            }
        );
        assert!(!differing[0].is_benign_table(&one, &three));
    }

    /// The benign cases over cells and tables, and the conflicts that are
    /// never benign.
    #[test]
    fn benign_table_conflicts_compare_new_values_and_nothing_else() {
        let alice = cells(&[("name", Value::Str("alice".into()))]);
        let first = TableDelta::identity()
            .with_deleted_row(RowId(0), "user", alice)
            .with_deleted_pair(fk(), RowId(2), RowId(0));
        let second = first.clone();
        let set = |n: f64| {
            TableDelta::identity()
                .with_cell_change(RowId(1), "n", None, Some(Value::Float(n)))
                .with_table_change(RowId(3), "user", "tag")
        };
        let (nan_a, nan_b) = (set(f64::NAN), set(f64::NAN));
        assert_eq!(nan_a.independence(&nan_b).len(), 2);
        assert!(
            nan_a
                .independence(&nan_b)
                .iter()
                .all(|conflict| conflict.is_benign_table(&nan_a, &nan_b))
        );
        let zero = set(0.0);
        let zeros = set(-0.0);
        let conflict = &zero.independence(&zeros)[0];
        assert!(matches!(
            conflict,
            EditConflict::UpdateUpdate {
                key: AttrKey::Cell { .. }
            }
        ));
        assert!(
            !conflict.is_benign_table(&zero, &zeros),
            "0.0 and -0.0 differ"
        );

        // Every other variant is never benign, nor is first graph reference.
        let moved = TableDelta::identity().with_table_change(RowId(0), "user", "tag");
        let linked = TableDelta::identity().with_added_pair(fk(), RowId(2), RowId(0));
        let anchor = moved.independence(&linked);
        assert_eq!(
            anchor,
            vec![EditConflict::AnchorUse {
                node: 0,
                user: ElemRef::Pair {
                    edge: Box::new(fk()),
                    src: RowId(2),
                    tgt: RowId(0)
                }
            }]
        );
        assert!(!anchor[0].is_benign_table(&moved, &linked));
        let use_ = first.independence(&TableDelta::identity().with_cell_change(
            RowId(0),
            "x",
            None,
            Some(Value::Int(1)),
        ));
        assert!(matches!(use_[0], EditConflict::DeleteUse { .. }));
        assert!(!use_[0].is_benign_table(&first, &first));
        let graph = EditConflict::DeleteDelete {
            elem: ElemRef::Node(0),
        };
        assert!(!graph.is_benign_table(&first, &second));
    }

    /// The canonical bytes and digest of one delta with every part, pinned.
    #[test]
    fn table_delta_bytes_are_pinned() {
        let delta = TableDelta::identity()
            .with_deleted_row(RowId(1), "user", cells(&[("a", Value::Int(1))]))
            .with_added_row(RowId(5), "post", Row::new())
            .with_deleted_pair(fk(), RowId(2), RowId(1))
            .with_added_pair(fk(), RowId(5), RowId(0))
            .with_cell_change(RowId(0), "name", None, Some(Value::Float(-0.0)))
            .with_table_change(RowId(3), "tag", "post");
        let mut writer = CanonicalWriter::new();
        canonical_table_delta_bytes(&delta, &mut writer);
        let hex = writer
            .as_bytes()
            .iter()
            .fold(String::new(), |mut hex, byte| {
                use std::fmt::Write as _;
                write!(hex, "{byte:02x}").unwrap();
                hex
            });
        assert_eq!(hex, PINNED_BYTES);
        assert_eq!(
            delta.digest(),
            Digest::derive(TABLE_DELTA_CONTEXT, writer.as_bytes())
        );
        assert_eq!(delta.digest().to_hex(), PINNED_DIGEST);
        // Equal deltas, built in another order, have equal bytes.
        let again = TableDelta::identity()
            .with_table_change(RowId(3), "tag", "post")
            .with_cell_change(RowId(0), "name", None, Some(Value::Float(-0.0)))
            .with_added_pair(fk(), RowId(5), RowId(0))
            .with_deleted_pair(fk(), RowId(2), RowId(1))
            .with_added_row(RowId(5), "post", Row::new())
            .with_deleted_row(RowId(1), "user", cells(&[("a", Value::Int(1))]));
        assert_eq!(again.digest(), delta.digest());
    }

    const PINNED_BYTES: &str = concat!(
        "01000000000000000100000000000000010000000000000001000000000000000100000000000000",
        "01000000000000000100000004000000000000007573657201000000000000000100000000000000",
        "61010100000000000000050000000400000000000000706f73740000000000000000040000000000",
        "0000706f73740400000000000000757365720200000000000000666b000200000001000000040000",
        "0000000000706f73740400000000000000757365720200000000000000666b000500000000000000",
        "0000000004000000000000006e616d65000102000000000000008003000000030000000000000074",
        "61670400000000000000706f7374",
    );
    const PINNED_DIGEST: &str = "95e7a5a78e435f2cea16f7f66662d0a089506996985727e55691a4a443bd6d35";
}
