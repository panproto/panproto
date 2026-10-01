//! Edit algebra for functor instances (model of `ThEditableStructure`).
//!
//! A [`TableEdit`] is an element of the edit monoid for relational
//! (table-shaped) instances. Every edit names the rows it touches by
//! [`RowId`] and carries what it expects to find there (compare and set), so
//! applying it checks the instance before changing anything. Every edit lowers
//! to one canonical [`TableDelta`] ([`TableEdit::to_delta`]), a batch to the
//! net delta of its steps, and its inverse is the inverse delta
//! ([`TableEdit::invert`]). The monoid operations are
//! [`TableEdit::identity`], [`TableEdit::compose`], and [`TableEdit::apply`]
//! (the partial monoid action on [`FInstance`]).
//!
//! The surface follows the asymmetric edit lenses of Johnson and Rosebrugh
//! 2016 (Section 5 onward, stated); its edits are invertible because each one
//! carries the values it replaces (inferred), not because the edit monoid of
//! Hofmann et al. 2012 has inverses, which it does not.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use crate::edit_error::EditError;
use crate::functor::{FInstance, ForeignKeyPair, Row};
use crate::graph_edit::ComposeError;
use crate::table_delta::{Pair, TableDelta, TableDeltaParts, compose_into, same_cell};
use crate::value::Value;
use crate::view_ref::RowId;

/// A model of `ThEditableStructure` for functor instances.
///
/// Serialized externally tagged, each variant's fields by name, every field
/// always written; a row's cells in ascending column order. Decoding is as
/// exact: every field is required (an absent `old` or `new` of a cell update
/// is an error, not `None`) and a field the variant does not have is
/// refused.
///
/// ```
/// use std::collections::HashMap;
///
/// use panproto_inst::{FInstance, RowId, TableEdit, Value};
///
/// let mut tables = FInstance::new();
/// let insert = TableEdit::InsertRow {
///     table: "user".into(),
///     id: RowId(0),
///     row: HashMap::from([("name".to_owned(), Value::Str("ada".into()))]),
/// };
/// insert.apply(&mut tables)?;
/// let rename = TableEdit::UpdateCell {
///     table: "user".into(),
///     id: RowId(0),
///     column: "name".into(),
///     old: Some(Value::Str("ada".into())),
///     new: Some(Value::Str("grace".into())),
/// };
/// rename.apply(&mut tables)?;
/// rename.invert()?.apply(&mut tables)?;
/// insert.invert()?.apply(&mut tables)?;
/// assert_eq!(tables.total_row_count(), 0);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TableEdit {
    /// The monoid identity: no change.
    Identity,

    /// Insert a row under an unused id.
    InsertRow {
        /// The table (schema vertex).
        table: Name,
        /// The new row's id.
        id: RowId,
        /// The row's cells.
        #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
        row: Row,
    },

    /// Delete a row together with the foreign-key pairs incident to it.
    DeleteRow {
        /// The table the row is expected in.
        table: Name,
        /// The row.
        id: RowId,
        /// The cells the row is expected to hold.
        #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
        row: Row,
        /// Every foreign-key pair incident to the row, in ascending order.
        incident_fks: Vec<ForeignKeyPair>,
    },

    /// Change one cell: `old: None` adds it, `new: None` removes it.
    UpdateCell {
        /// The table the row is expected in.
        table: Name,
        /// The row.
        id: RowId,
        /// The column.
        column: String,
        /// The cell's expected value, `None` when it is expected absent.
        /// Required on decode: a missing `old` is refused, not read as
        /// `None`.
        #[serde(deserialize_with = "Option::deserialize")]
        old: Option<Value>,
        /// The cell's new value, `None` to remove it. Required on decode.
        #[serde(deserialize_with = "Option::deserialize")]
        new: Option<Value>,
    },

    /// Move a row to another table, keeping its id and cells.
    MoveRow {
        /// The row.
        id: RowId,
        /// The table the row is expected in.
        old_table: Name,
        /// The table it moves to.
        new_table: Name,
    },

    /// Add a pair to a foreign key.
    AddForeignKey {
        /// The foreign-key edge.
        edge: Edge,
        /// The source row.
        src: RowId,
        /// The target row.
        tgt: RowId,
    },

    /// Remove a pair from a foreign key.
    RemoveForeignKey {
        /// The foreign-key edge.
        edge: Edge,
        /// The source row.
        src: RowId,
        /// The target row.
        tgt: RowId,
    },

    /// A sequence of edits applied in order, all or nothing.
    Batch(Vec<Self>),

    /// A canonical table delta.
    Delta(TableDelta),
}

impl TableEdit {
    /// The monoid identity element.
    #[must_use]
    pub const fn identity() -> Self {
        Self::Identity
    }

    /// Monoid multiplication: `self` then `other`, as one batch.
    ///
    /// Nested batches are flattened and identity elements are elided.
    #[must_use]
    pub fn compose(self, other: Self) -> Self {
        let mut steps = Vec::new();
        flatten_into(&mut steps, self);
        flatten_into(&mut steps, other);
        match steps.len() {
            0 => Self::Identity,
            1 => steps.into_iter().next().unwrap_or(Self::Identity),
            _ => Self::Batch(steps),
        }
    }

    /// Returns `true` if this edit is the identity (no-op).
    #[must_use]
    pub fn is_identity(&self) -> bool {
        match self {
            Self::Identity => true,
            Self::Batch(steps) => steps.iter().all(Self::is_identity),
            Self::Delta(delta) => delta.is_identity(),
            _ => false,
        }
    }

    /// The canonical delta of this edit, without the instance.
    ///
    /// `InsertRow` adds the row; `DeleteRow` deletes the row and its listed
    /// pairs; `UpdateCell` is a cell change; `MoveRow` a table change of the
    /// row; `AddForeignKey` and `RemoveForeignKey` add and delete the pair;
    /// `Delta` is its delta; `Batch` is the left-to-right composite of its
    /// steps ([`TableDelta::compose`]), one net delta, computed in time
    /// linear in the steps (each composed in place onto the running
    /// composite).
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError::Mismatch`] when the steps of a batch do not
    /// compose.
    pub fn to_delta(&self) -> Result<TableDelta, ComposeError> {
        let mut parts = TableDeltaParts::default();
        lower_into(self, &mut parts)?;
        Ok(TableDelta::normalize(parts))
    }

    /// The inverse edit: `Delta(to_delta()?.invert())`. Applying an edit
    /// and then its inverse to an instance the edit applies to gives back
    /// that instance. The inverse of a `DeleteRow` re-inserts the row and
    /// then its incident pairs: it is the delta of
    /// `Batch[InsertRow, AddForeignKey*]`, the pairs in ascending order.
    ///
    /// # Errors
    ///
    /// As [`Self::to_delta`].
    pub fn invert(&self) -> Result<Self, ComposeError> {
        Ok(Self::Delta(self.to_delta()?.invert()))
    }

    /// Apply this edit to a functor instance.
    ///
    /// Each step is checked against the state it meets, the instance
    /// followed by the net delta of the steps before it, without changing
    /// anything: an `InsertRow` needs its id free ([`EditError::DuplicateRow`]);
    /// a `DeleteRow`, `UpdateCell` or `MoveRow` needs its row
    /// ([`EditError::RowNotFound`]) in the table it names
    /// ([`EditError::StaleTable`]); a `DeleteRow` needs the row's cells
    /// ([`EditError::StaleCell`], the least differing column) and its listed
    /// pairs to be exactly the pairs at the row
    /// ([`EditError::StaleIncidence`]); an `UpdateCell` needs the cell's old
    /// value ([`EditError::StaleCell`]); a `MoveRow` to another table needs a
    /// row without pairs ([`EditError::MissingRowEndpoint`], naming the first
    /// pair's edge); an `AddForeignKey` needs the pair absent
    /// ([`EditError::DuplicateForeignKey`]) and its rows in the edge's
    /// endpoint tables ([`EditError::MissingRowEndpoint`]); a
    /// `RemoveForeignKey` needs the pair present
    /// ([`EditError::ForeignKeyNotFound`]). A `Delta` step is checked as
    /// part of the net delta. The net delta of the whole edit is then
    /// applied ([`TableDelta::apply`]). Cells and rows compare by canonical
    /// bytes, so a `NaN` cell matches itself and `0.0` does not match
    /// `-0.0`.
    ///
    /// On error the instance is unchanged and is not copied, except that a
    /// batch whose net delta fails is replayed on a copy to name the failing
    /// step. A failing step of a batch is reported as
    /// [`EditError::BatchStep`] with its index (nested for nested batches).
    ///
    /// # Errors
    ///
    /// The first failing check's error, as above; [`EditError::Compose`]
    /// when the steps around a `Delta` step do not compose; and the errors
    /// of [`TableDelta::apply`].
    pub fn apply(&self, instance: &mut FInstance) -> Result<(), EditError> {
        let mut prefix = Prefix::new(instance);
        let mut path = Vec::new();
        if let Err(err) = check_into(self, &mut prefix, &mut path) {
            return Err(at_path(&path, err));
        }
        let delta = TableDelta::normalize(prefix.parts);
        match delta.apply(instance) {
            Ok(()) => Ok(()),
            Err(err) => Err(self.diagnose(instance, err)),
        }
    }

    /// Name the failing step of a batch by replaying its steps on a copy;
    /// `err` when the edit is not a batch or no step fails on its own.
    fn diagnose(&self, instance: &FInstance, err: EditError) -> EditError {
        let Self::Batch(steps) = self else {
            return err;
        };
        let mut replay = instance.clone();
        for (index, step) in steps.iter().enumerate() {
            if let Err(source) = step.apply(&mut replay) {
                return EditError::BatchStep {
                    index,
                    source: Box::new(source),
                };
            }
        }
        err
    }
}

/// The delta of one step that is neither a batch nor a delta.
fn step_delta(edit: &TableEdit) -> TableDelta {
    let id = TableDelta::identity;
    match edit {
        TableEdit::InsertRow {
            table,
            id: row,
            row: cells,
        } => id().with_added_row(*row, table.clone(), cells.clone()),
        TableEdit::DeleteRow {
            table,
            id: row,
            row: cells,
            incident_fks,
        } => incident_fks.iter().fold(
            id().with_deleted_row(*row, table.clone(), cells.clone()),
            |delta, (edge, src, tgt)| delta.with_deleted_pair(edge.clone(), *src, *tgt),
        ),
        TableEdit::UpdateCell {
            id: row,
            column,
            old,
            new,
            ..
        } => id().with_cell_change(*row, column.clone(), old.clone(), new.clone()),
        TableEdit::MoveRow {
            id: row,
            old_table,
            new_table,
        } => id().with_table_change(*row, old_table.clone(), new_table.clone()),
        TableEdit::AddForeignKey { edge, src, tgt } => {
            id().with_added_pair(edge.clone(), *src, *tgt)
        }
        TableEdit::RemoveForeignKey { edge, src, tgt } => {
            id().with_deleted_pair(edge.clone(), *src, *tgt)
        }
        TableEdit::Identity | TableEdit::Batch(_) | TableEdit::Delta(_) => id(),
    }
}

/// Compose the delta of `edit` onto `parts` in place.
fn lower_into(edit: &TableEdit, parts: &mut TableDeltaParts) -> Result<(), ComposeError> {
    match edit {
        TableEdit::Identity => Ok(()),
        TableEdit::Batch(steps) => steps.iter().try_for_each(|step| lower_into(step, parts)),
        TableEdit::Delta(delta) => compose_into(parts, delta),
        step => compose_into(parts, &step_delta(step)),
    }
}

/// `err` inside the batch steps `path` names, outermost first.
fn at_path(path: &[usize], err: EditError) -> EditError {
    path.iter()
        .rev()
        .fold(err, |source, &index| EditError::BatchStep {
            index,
            source: Box::new(source),
        })
}

/// The state a step of an edit meets: the instance followed by the net delta
/// of the steps before it, read without applying it.
struct Prefix<'a> {
    instance: &'a FInstance,
    parts: TableDeltaParts,
    /// The pairs the net delta adds, by each of their two rows.
    added_at: BTreeMap<RowId, BTreeSet<Pair>>,
}

impl<'a> Prefix<'a> {
    fn new(instance: &'a FInstance) -> Self {
        Self {
            instance,
            parts: TableDeltaParts::default(),
            added_at: BTreeMap::new(),
        }
    }

    /// The table of row `id`, `None` when there is no such row.
    fn table(&self, id: RowId) -> Option<&Name> {
        if let Some((table, _)) = self.parts.add_rows.get(&id) {
            return Some(table);
        }
        if self.parts.del_rows.contains_key(&id) {
            return None;
        }
        self.parts
            .tables
            .get(&id)
            .map(|change| &change.new)
            .or_else(|| self.instance.table_of(id))
    }

    /// The cells of row `id`, `None` when there is no such row.
    fn row(&self, id: RowId) -> Option<Row> {
        if let Some((_, cells)) = self.parts.add_rows.get(&id) {
            return Some(cells.clone());
        }
        if self.parts.del_rows.contains_key(&id) {
            return None;
        }
        let mut cells = self.instance.row(id)?.clone();
        for ((row, column), change) in self.parts.cells.range((id, String::new())..) {
            if *row != id {
                break;
            }
            match &change.new {
                Some(value) => {
                    cells.insert(column.clone(), value.clone());
                }
                None => {
                    cells.remove(column);
                }
            }
        }
        Some(cells)
    }

    /// Whether the pair is present.
    fn has_pair(&self, pair: &Pair) -> bool {
        self.parts.add_pairs.contains(pair)
            || (!self.parts.del_pairs.contains(pair)
                && self.instance.contains_pair(&pair.0, pair.1, pair.2))
    }

    /// The pairs at row `id`, ascending.
    fn incident(&self, id: RowId) -> Vec<Pair> {
        let mut pairs: BTreeSet<Pair> = self
            .instance
            .incident_pairs(id)
            .into_iter()
            .filter(|pair| self.has_pair(pair))
            .collect();
        if let Some(added) = self.added_at.get(&id) {
            pairs.extend(added.iter().cloned());
        }
        pairs.into_iter().collect()
    }

    /// Follow one more delta.
    fn absorb(&mut self, delta: &TableDelta) -> Result<(), ComposeError> {
        compose_into(&mut self.parts, delta)?;
        for pair in delta.touched_pairs() {
            let added = self.parts.add_pairs.contains(&pair);
            for row in [pair.1, pair.2] {
                let at = self.added_at.entry(row).or_default();
                if added {
                    at.insert(pair.clone());
                } else {
                    at.remove(&pair);
                }
            }
        }
        Ok(())
    }

    /// The row's table, which must exist and be `table`.
    fn expect_table(&self, id: RowId, table: &Name) -> Result<(), EditError> {
        match self.table(id) {
            None => Err(EditError::RowNotFound(id)),
            Some(actual) if actual != table => Err(EditError::StaleTable { row: id }),
            Some(_) => Ok(()),
        }
    }

    /// Check one step that is neither a batch nor a delta, as
    /// [`TableEdit::apply`] lists the checks.
    fn check_step(&self, step: &TableEdit) -> Result<(), EditError> {
        match step {
            TableEdit::InsertRow { id, .. } => {
                if self.table(*id).is_some() {
                    return Err(EditError::DuplicateRow(*id));
                }
            }
            TableEdit::DeleteRow {
                table,
                id,
                row,
                incident_fks,
            } => {
                self.expect_table(*id, table)?;
                let cells = self.row(*id).ok_or(EditError::RowNotFound(*id))?;
                let columns: BTreeSet<&String> = cells.keys().chain(row.keys()).collect();
                if let Some(column) = columns
                    .into_iter()
                    .find(|c| !same_cell(cells.get(*c), row.get(*c)))
                {
                    return Err(EditError::StaleCell {
                        row: *id,
                        column: column.clone(),
                    });
                }
                if self.incident(*id) != *incident_fks {
                    return Err(EditError::StaleIncidence { row: *id });
                }
            }
            TableEdit::UpdateCell {
                table,
                id,
                column,
                old,
                ..
            } => {
                self.expect_table(*id, table)?;
                let cells = self.row(*id).ok_or(EditError::RowNotFound(*id))?;
                if !same_cell(cells.get(column), old.as_ref()) {
                    return Err(EditError::StaleCell {
                        row: *id,
                        column: column.clone(),
                    });
                }
            }
            TableEdit::MoveRow {
                id,
                old_table,
                new_table,
            } => {
                self.expect_table(*id, old_table)?;
                if old_table != new_table
                    && let Some((edge, _, _)) = self.incident(*id).into_iter().next()
                {
                    return Err(EditError::MissingRowEndpoint { edge, row: *id });
                }
            }
            TableEdit::AddForeignKey { edge, src, tgt } => {
                let pair = (edge.clone(), *src, *tgt);
                if self.has_pair(&pair) {
                    return Err(EditError::DuplicateForeignKey {
                        edge: edge.clone(),
                        src: *src,
                        tgt: *tgt,
                    });
                }
                for (row, table) in [(*src, &edge.src), (*tgt, &edge.tgt)] {
                    if self.table(row) != Some(table) {
                        return Err(EditError::MissingRowEndpoint {
                            edge: edge.clone(),
                            row,
                        });
                    }
                }
            }
            TableEdit::RemoveForeignKey { edge, src, tgt } => {
                if !self.has_pair(&(edge.clone(), *src, *tgt)) {
                    return Err(EditError::ForeignKeyNotFound {
                        edge: edge.clone(),
                        src: *src,
                        tgt: *tgt,
                    });
                }
            }
            TableEdit::Identity | TableEdit::Batch(_) | TableEdit::Delta(_) => {}
        }
        Ok(())
    }
}

/// Check every step of `edit` against the state it meets and fold its delta
/// into `prefix`; on error, `path` holds the batch indices of the failing
/// step.
fn check_into(
    edit: &TableEdit,
    prefix: &mut Prefix<'_>,
    path: &mut Vec<usize>,
) -> Result<(), EditError> {
    match edit {
        TableEdit::Identity => Ok(()),
        TableEdit::Batch(steps) => {
            for (index, step) in steps.iter().enumerate() {
                path.push(index);
                check_into(step, prefix, path)?;
                path.pop();
            }
            Ok(())
        }
        TableEdit::Delta(delta) => prefix.absorb(delta).map_err(EditError::Compose),
        step => {
            prefix.check_step(step)?;
            prefix.absorb(&step_delta(step)).map_err(EditError::Compose)
        }
    }
}

/// Flatten nested batches and strip identities.
fn flatten_into(out: &mut Vec<TableEdit>, edit: TableEdit) {
    match edit {
        TableEdit::Identity => {}
        TableEdit::Batch(steps) => {
            for step in steps {
                flatten_into(out, step);
            }
        }
        other => out.push(other),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;

    use proptest::prelude::*;

    use super::*;
    use crate::error::InstError;
    use crate::table_delta::TableDelta;
    use crate::table_delta::tests::{TestResult, cells};

    fn fk() -> Edge {
        Edge {
            src: "post".into(),
            tgt: "user".into(),
            kind: "fk".into(),
            name: None,
        }
    }

    /// [`TableDelta`] serializes its fields in the order and with the
    /// types the contract states, in every wire format.
    #[test]
    fn table_delta_fields_serialize_in_contract_order() -> TestResult {
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
        let json = serde_json::to_string(&delta)?;
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
        let back: TableDelta = serde_json::from_str(&json)?;
        assert_eq!(back, delta);
        let packed = rmp_serde::to_vec_named(&delta)?;
        assert_eq!(rmp_serde::from_slice::<TableDelta>(&packed)?, delta);
        let mut cbor = Vec::new();
        ciborium::into_writer(&delta, &mut cbor)?;
        assert_eq!(
            ciborium::from_reader::<TableDelta, _>(cbor.as_slice())?,
            delta
        );

        // A cell change spells out both values; decoding normalizes.
        let missing_old = r#"{"del_rows":{},"add_rows":{},"del_pairs":[],"add_pairs":[],"cells":[[[0,"n"],{"new":null}]],"tables":{}}"#;
        let err = serde_json::from_str::<TableDelta>(missing_old)
            .err()
            .ok_or("expected an error")?;
        assert!(err.to_string().contains("missing field `old`"), "{err}");
        let no_op = r#"{"del_rows":{},"add_rows":{},"del_pairs":[],"add_pairs":[],"cells":[[[0,"n"],{"old":{"Int":1},"new":{"Int":1}}]],"tables":{"4":{"old":"a","new":"a"}}}"#;
        assert!(serde_json::from_str::<TableDelta>(no_op)?.is_identity());
        Ok(())
    }

    fn named(value: &str) -> Row {
        HashMap::from([("name".to_owned(), Value::Str(value.into()))])
    }

    /// Users 0 (alice) and 1 (bob), post 2 by alice.
    fn sample_instance() -> FInstance {
        FInstance::new()
            .with_table("user", vec![named("alice"), named("bob")])
            .and_then(|f| f.with_table("post", vec![named("hello")]))
            .and_then(|f| f.with_foreign_key(fk(), [(RowId(2), RowId(0))]))
            .unwrap()
    }

    #[test]
    fn identity_is_noop() {
        let mut inst = sample_instance();
        TableEdit::identity().apply(&mut inst).unwrap();
        assert_eq!(inst, sample_instance());
    }

    #[test]
    fn insert_row_refuses_an_id_in_use() {
        let mut inst = sample_instance();
        let edit = TableEdit::InsertRow {
            table: "user".into(),
            id: RowId(2),
            row: named("carol"),
        };
        assert_eq!(
            edit.apply(&mut inst),
            Err(EditError::DuplicateRow(RowId(2)))
        );
        assert_eq!(inst, sample_instance(), "a refused edit changes nothing");
    }

    #[test]
    fn delete_row_takes_its_incident_pairs() {
        let mut inst = sample_instance();
        let edit = TableEdit::DeleteRow {
            table: "user".into(),
            id: RowId(0),
            row: named("alice"),
            incident_fks: vec![(fk(), RowId(2), RowId(0))],
        };
        edit.apply(&mut inst).unwrap();
        assert_eq!(inst.row_count("user"), 1);
        assert_eq!(inst.pair_count(), 0);
        edit.invert().unwrap().apply(&mut inst).unwrap();
        assert_eq!(inst, sample_instance());
    }

    #[test]
    fn delete_row_with_a_stale_incident_set_is_refused() {
        let mut inst = sample_instance();
        let stale = TableEdit::DeleteRow {
            table: "user".into(),
            id: RowId(0),
            row: named("alice"),
            incident_fks: Vec::new(),
        };
        assert_eq!(
            stale.apply(&mut inst),
            Err(EditError::StaleIncidence { row: RowId(0) })
        );
        let wrong_cells = TableEdit::DeleteRow {
            table: "user".into(),
            id: RowId(0),
            row: named("alicia"),
            incident_fks: vec![(fk(), RowId(2), RowId(0))],
        };
        assert_eq!(
            wrong_cells.apply(&mut inst),
            Err(EditError::StaleCell {
                row: RowId(0),
                column: "name".into()
            })
        );
        let wrong_table = TableEdit::DeleteRow {
            table: "post".into(),
            id: RowId(0),
            row: named("alice"),
            incident_fks: vec![(fk(), RowId(2), RowId(0))],
        };
        assert_eq!(
            wrong_table.apply(&mut inst),
            Err(EditError::StaleTable { row: RowId(0) })
        );
        assert_eq!(inst, sample_instance());
    }

    #[test]
    fn update_cell_adds_and_removes_by_option() {
        let mut inst = sample_instance();
        let add = TableEdit::UpdateCell {
            table: "user".into(),
            id: RowId(1),
            column: "age".into(),
            old: None,
            new: Some(Value::Int(40)),
        };
        add.apply(&mut inst).unwrap();
        assert_eq!(
            inst.row(RowId(1)).unwrap().get("age"),
            Some(&Value::Int(40))
        );
        // Adding again is stale: the cell is no longer absent.
        assert_eq!(
            add.apply(&mut inst),
            Err(EditError::StaleCell {
                row: RowId(1),
                column: "age".into()
            })
        );
        let remove = TableEdit::UpdateCell {
            table: "user".into(),
            id: RowId(1),
            column: "name".into(),
            old: Some(Value::Str("bob".into())),
            new: None,
        };
        remove.apply(&mut inst).unwrap();
        assert!(!inst.row(RowId(1)).unwrap().contains_key("name"));
        remove.invert().unwrap().apply(&mut inst).unwrap();
        add.invert().unwrap().apply(&mut inst).unwrap();
        assert_eq!(inst, sample_instance());
    }

    #[test]
    fn a_failing_batch_changes_nothing() {
        let mut inst = sample_instance();
        let batch = TableEdit::Batch(vec![
            TableEdit::InsertRow {
                table: "user".into(),
                id: RowId(9),
                row: named("dan"),
            },
            TableEdit::RemoveForeignKey {
                edge: fk(),
                src: RowId(2),
                tgt: RowId(1),
            },
        ]);
        assert!(batch.apply(&mut inst).is_err());
        assert_eq!(inst, sample_instance());
    }

    /// The exact bytes of an instance: equality that holds a `NaN` cell
    /// equal to itself and tells `0.0` from `-0.0`.
    fn bytes(instance: &FInstance) -> Vec<u8> {
        crate::functor::canonical_finstance_bytes(instance)
    }

    #[test]
    fn an_edit_naming_a_nan_cell_applies_and_inverts() {
        let mut inst = FInstance::new();
        let insert = TableEdit::InsertRow {
            table: "t".into(),
            id: RowId(0),
            row: HashMap::from([("x".to_owned(), Value::Float(f64::NAN))]),
        };
        insert.apply(&mut inst).unwrap();
        let update = TableEdit::UpdateCell {
            table: "t".into(),
            id: RowId(0),
            column: "x".into(),
            old: Some(Value::Float(f64::NAN)),
            new: Some(Value::Int(1)),
        };
        let with_nan = bytes(&inst);
        update.apply(&mut inst).unwrap();
        update.invert().unwrap().apply(&mut inst).unwrap();
        assert_eq!(bytes(&inst), with_nan);
        insert.invert().unwrap().apply(&mut inst).unwrap();
        assert_eq!(inst.total_row_count(), 0);
    }

    /// Compare and set reads a cell's exact value: an update expecting `0.0`
    /// does not apply to `-0.0`, since its inverse would then write back a
    /// different value than the one it replaced.
    #[test]
    fn compare_and_set_tells_signed_zeros_apart() {
        let mut inst = FInstance::new()
            .with_table(
                "t",
                vec![HashMap::from([("x".to_owned(), Value::Float(-0.0))])],
            )
            .unwrap();
        let before = bytes(&inst);
        let update = TableEdit::UpdateCell {
            table: "t".into(),
            id: RowId(0),
            column: "x".into(),
            old: Some(Value::Float(0.0)),
            new: Some(Value::Int(1)),
        };
        assert_eq!(
            update.apply(&mut inst),
            Err(EditError::StaleCell {
                row: RowId(0),
                column: "x".into()
            })
        );
        let delete = TableEdit::DeleteRow {
            table: "t".into(),
            id: RowId(0),
            row: HashMap::from([("x".to_owned(), Value::Float(0.0))]),
            incident_fks: Vec::new(),
        };
        assert_eq!(
            delete.apply(&mut inst),
            Err(EditError::StaleCell {
                row: RowId(0),
                column: "x".into()
            })
        );
        assert_eq!(bytes(&inst), before);
    }

    /// Undoing a bulk delete is linear: a batch applies in place and does
    /// not copy the instance, so the inverse of `k` deletions (a batch of
    /// `k` batches) costs as much as the deletions.
    #[test]
    fn undoing_a_bulk_delete_is_linear() {
        const ROWS: u32 = 8000;
        let rows = (0..ROWS).map(|i| HashMap::from([("n".to_owned(), Value::Int(i64::from(i)))]));
        let start = FInstance::new().with_table("t", rows).unwrap();
        let deletes = TableEdit::Batch(
            (0..ROWS)
                .map(|i| TableEdit::DeleteRow {
                    table: "t".into(),
                    id: RowId(i),
                    row: start.row(RowId(i)).unwrap().clone(),
                    incident_fks: Vec::new(),
                })
                .collect(),
        );
        let mut state = start.clone();
        deletes.apply(&mut state).unwrap();
        assert_eq!(state.total_row_count(), 0);
        let began = std::time::Instant::now();
        deletes.invert().unwrap().apply(&mut state).unwrap();
        let took = began.elapsed();
        assert_eq!(state, start);
        assert!(
            took < std::time::Duration::from_secs(2),
            "undoing {ROWS} deletions took {took:?}"
        );
    }

    /// A failing step deep in a nested batch leaves the instance as it was,
    /// although the batch applies in place.
    #[test]
    fn a_failing_nested_batch_rolls_back_every_step() {
        let mut inst = sample_instance();
        let before = bytes(&inst);
        let batch = TableEdit::Batch(vec![
            TableEdit::Batch(vec![
                TableEdit::InsertRow {
                    table: "post".into(),
                    id: RowId(9),
                    row: named("draft"),
                },
                TableEdit::AddForeignKey {
                    edge: fk(),
                    src: RowId(9),
                    tgt: RowId(1),
                },
            ]),
            TableEdit::UpdateCell {
                table: "user".into(),
                id: RowId(0),
                column: "name".into(),
                old: Some(Value::Str("alice".into())),
                new: None,
            },
            TableEdit::DeleteRow {
                table: "post".into(),
                id: RowId(2),
                row: named("hello"),
                incident_fks: vec![(fk(), RowId(2), RowId(0))],
            },
            TableEdit::Batch(vec![TableEdit::RemoveForeignKey {
                edge: fk(),
                src: RowId(2),
                tgt: RowId(1),
            }]),
        ]);
        assert!(batch.apply(&mut inst).is_err());
        assert_eq!(bytes(&inst), before);
        assert_eq!(inst, sample_instance());
    }

    #[test]
    fn table_edit_variants_serialize_with_contract_names() {
        let edits = vec![
            TableEdit::Identity,
            TableEdit::InsertRow {
                table: "user".into(),
                id: RowId(3),
                row: HashMap::from([
                    ("b".to_owned(), Value::Int(2)),
                    ("a".to_owned(), Value::Int(1)),
                ]),
            },
            TableEdit::DeleteRow {
                table: "user".into(),
                id: RowId(0),
                row: named("alice"),
                incident_fks: vec![(fk(), RowId(2), RowId(0))],
            },
            TableEdit::UpdateCell {
                table: "user".into(),
                id: RowId(1),
                column: "name".into(),
                old: None,
                new: None,
            },
            TableEdit::MoveRow {
                id: RowId(1),
                old_table: "user".into(),
                new_table: "admin".into(),
            },
            TableEdit::AddForeignKey {
                edge: fk(),
                src: RowId(2),
                tgt: RowId(1),
            },
            TableEdit::RemoveForeignKey {
                edge: fk(),
                src: RowId(2),
                tgt: RowId(0),
            },
            TableEdit::Batch(vec![TableEdit::Identity]),
            TableEdit::Delta(TableDelta::identity().with_table_change(RowId(4), "a", "b")),
        ];
        let json = serde_json::to_value(&edits).unwrap();
        let fk_json = serde_json::json!({"src": "post", "tgt": "user", "kind": "fk", "name": null});
        assert_eq!(
            json,
            serde_json::json!([
                "Identity",
                {"InsertRow": {"table": "user", "id": 3, "row": {"a": {"Int": 1}, "b": {"Int": 2}}}},
                {"DeleteRow": {"table": "user", "id": 0, "row": {"name": {"Str": "alice"}},
                    "incident_fks": [[fk_json, 2, 0]]}},
                {"UpdateCell": {"table": "user", "id": 1, "column": "name", "old": null, "new": null}},
                {"MoveRow": {"id": 1, "old_table": "user", "new_table": "admin"}},
                {"AddForeignKey": {"edge": fk_json, "src": 2, "tgt": 1}},
                {"RemoveForeignKey": {"edge": fk_json, "src": 2, "tgt": 0}},
                {"Batch": ["Identity"]},
                {"Delta": {"del_rows": {}, "add_rows": {}, "del_pairs": [], "add_pairs": [],
                    "cells": [], "tables": {"4": {"old": "a", "new": "b"}}}},
            ])
        );
        let back: Vec<TableEdit> = serde_json::from_value(json).unwrap();
        assert_eq!(back, edits);
    }

    /// `old: None` means the cell is expected absent, so a cell update
    /// without its `old` (or `new`) field is refused rather than read as an
    /// addition (or a removal), and a field the edit does not have is
    /// refused rather than ignored.
    #[test]
    fn a_cell_update_must_spell_out_both_values() {
        let decode = serde_json::from_str::<TableEdit>;
        // The decode error of `json`, or the edit it wrongly decoded to.
        let refusal = |json: &str| match serde_json::from_str::<TableEdit>(json) {
            Ok(edit) => format!("decoded to {edit:?}"),
            Err(err) => err.to_string(),
        };
        let missing_old = r#"{"UpdateCell":{"table":"t","id":0,"column":"n","new":null}}"#;
        let err = refusal(missing_old);
        assert!(err.contains("missing field `old`"), "{err}");
        let missing_new = r#"{"UpdateCell":{"table":"t","id":0,"column":"n","old":{"Int":1}}}"#;
        let err = refusal(missing_new);
        assert!(err.contains("missing field `new`"), "{err}");
        let unknown =
            r#"{"UpdateCell":{"table":"t","id":0,"column":"n","old":null,"new":null,"od":null}}"#;
        let err = refusal(unknown);
        assert!(err.contains("unknown field `od`"), "{err}");
        let unknown = r#"{"MoveRow":{"id":0,"old_table":"a","new_table":"b","table":"a"}}"#;
        let err = refusal(unknown);
        assert!(err.contains("unknown field `table`"), "{err}");

        let spelled =
            r#"{"UpdateCell":{"table":"t","id":0,"column":"n","old":null,"new":{"Int":1}}}"#;
        assert_eq!(
            decode(spelled).unwrap(),
            TableEdit::UpdateCell {
                table: "t".into(),
                id: RowId(0),
                column: "n".into(),
                old: None,
                new: Some(Value::Int(1)),
            }
        );
    }

    /// An edit that creates a table is undone by its inverse: the table it
    /// created goes away with its last row.
    #[test]
    fn an_edit_that_creates_a_table_is_undone_by_its_inverse() {
        let start = FInstance::new();
        let insert = TableEdit::InsertRow {
            table: "user".into(),
            id: RowId(0),
            row: Row::new(),
        };
        let mut state = start.clone();
        insert.apply(&mut state).unwrap();
        insert.invert().unwrap().apply(&mut state).unwrap();
        assert_eq!(state, start);
        assert_eq!(state.table_count(), 0);

        let start = FInstance::new()
            .with_rows("a", [(RowId(0), Row::new())])
            .unwrap();
        let moved = TableEdit::MoveRow {
            id: RowId(0),
            old_table: "a".into(),
            new_table: "b".into(),
        };
        let mut state = start.clone();
        moved.apply(&mut state).unwrap();
        moved.invert().unwrap().apply(&mut state).unwrap();
        assert_eq!(state, start);
        assert_eq!(state.table_count(), 1);
    }

    /// Moving a row that takes part in a foreign-key pair is refused with an
    /// error naming the pair's edge and the row it would leave outside the
    /// edge's endpoint table, not one claiming the row is missing from its
    /// table, and leaves the instance unchanged.
    #[test]
    fn moving_a_row_with_a_foreign_key_pair_is_refused_for_the_pair() {
        let fk = Edge {
            src: "post".into(),
            tgt: "user".into(),
            kind: "fk".into(),
            name: None,
        };
        let mut start = FInstance::new()
            .with_rows("user", [(RowId(3), Row::new())])
            .unwrap()
            .with_rows("post", [(RowId(1), Row::new())])
            .unwrap();
        start
            .insert_foreign_key(fk.clone(), RowId(1), RowId(3))
            .unwrap();
        let mut state = start.clone();
        let err = TableEdit::MoveRow {
            id: RowId(3),
            old_table: "user".into(),
            new_table: "member".into(),
        }
        .apply(&mut state)
        .unwrap_err();
        assert_eq!(
            err,
            EditError::MissingRowEndpoint {
                edge: fk.clone(),
                row: RowId(3)
            }
        );
        assert_eq!(state, start);
        assert_eq!(state.table_of(RowId(3)), Some(&Name::from("user")));

        let refusal = state.move_row(RowId(3), "member").unwrap_err();
        assert!(
            matches!(
                &refusal,
                InstError::RowHasForeignKeys { row: RowId(3), from, to, edge, src: RowId(1), tgt: RowId(3) }
                    if from == "user" && to == "member" && *edge == fk
            ),
            "expected RowHasForeignKeys, got {refusal:?}"
        );
        assert_eq!(state, start);
    }

    /// The tables a generated script draws from; `tag` starts absent.
    const TABLES: [&str; 3] = ["user", "post", "tag"];

    /// One step of a generated edit script, resolved against the state it
    /// applies to.
    #[derive(Clone, Debug)]
    enum Intent {
        Insert(u8),
        Delete(u8),
        Update(u8, Option<Value>),
        Move(u8, u8),
        Link(u8, u8),
        Unlink(u8),
    }

    fn intent() -> impl Strategy<Value = Intent> {
        prop_oneof![
            any::<u8>().prop_map(Intent::Insert),
            any::<u8>().prop_map(Intent::Delete),
            (any::<u8>(), proptest::option::of(cell_value()))
                .prop_map(|(r, v)| Intent::Update(r, v)),
            (any::<u8>(), any::<u8>()).prop_map(|(r, to)| Intent::Move(r, to)),
            (any::<u8>(), any::<u8>()).prop_map(|(a, b)| Intent::Link(a, b)),
            any::<u8>().prop_map(Intent::Unlink),
        ]
    }

    /// Cell values, including the floats whose `==` is not an identity:
    /// `NaN` and the two zeros.
    fn cell_value() -> impl Strategy<Value = Value> {
        prop_oneof![
            (-3_i64..3).prop_map(Value::Int),
            Just(Value::Float(f64::NAN)),
            Just(Value::Float(0.0)),
            Just(Value::Float(-0.0)),
            Just(Value::Float(f64::INFINITY)),
            Just(Value::List(vec![Value::Float(f64::NAN)])),
        ]
    }

    fn pick<T: Clone>(items: &[T], at: u8) -> Option<T> {
        (!items.is_empty()).then(|| items[usize::from(at) % items.len()].clone())
    }

    /// The concrete edit an intent names in `state`, when there is one.
    fn resolve(state: &FInstance, intent: &Intent) -> Option<TableEdit> {
        let rows = state.row_ids();
        match intent {
            Intent::Insert(t) => Some(TableEdit::InsertRow {
                table: TABLES[usize::from(*t) % TABLES.len()].into(),
                id: state.fresh_row_id().ok()?,
                row: HashMap::from([("n".to_owned(), Value::Int(i64::from(*t)))]),
            }),
            Intent::Delete(r) => {
                let id = pick(&rows, *r)?;
                Some(TableEdit::DeleteRow {
                    table: state.table_of(id)?.clone(),
                    id,
                    row: state.row(id)?.clone(),
                    incident_fks: state.incident_pairs(id),
                })
            }
            Intent::Update(r, value) => {
                let id = pick(&rows, *r)?;
                Some(TableEdit::UpdateCell {
                    table: state.table_of(id)?.clone(),
                    id,
                    column: "n".into(),
                    old: state.row(id)?.get("n").cloned(),
                    new: value.clone(),
                })
            }
            Intent::Move(r, to) => {
                let free: Vec<RowId> = rows
                    .into_iter()
                    .filter(|id| state.incident_pairs(*id).is_empty())
                    .collect();
                let id = pick(&free, *r)?;
                let old_table = state.table_of(id)?.clone();
                let others: Vec<&str> = TABLES
                    .into_iter()
                    .filter(|t| *t != old_table.as_ref())
                    .collect();
                let new_table = pick(&others, *to)?;
                Some(TableEdit::MoveRow {
                    id,
                    old_table,
                    new_table: new_table.into(),
                })
            }
            Intent::Link(a, b) => {
                let posts = state.table("post")?.ids();
                let users = state.table("user")?.ids();
                let (src, tgt) = (pick(&posts, *a)?, pick(&users, *b)?);
                (!state.contains_pair(&fk(), src, tgt)).then(|| TableEdit::AddForeignKey {
                    edge: fk(),
                    src,
                    tgt,
                })
            }
            Intent::Unlink(p) => {
                let (edge, src, tgt) = pick(&state.all_pairs(), *p)?;
                Some(TableEdit::RemoveForeignKey { edge, src, tgt })
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn a_table_edit_then_its_inverse_is_the_identity(
            from_empty in any::<bool>(),
            intents in proptest::collection::vec(intent(), 0..24)
        ) {
            let start = if from_empty { FInstance::new() } else { sample_instance() };
            let mut state = start.clone();
            let mut applied = Vec::new();
            for intent in &intents {
                let Some(edit) = resolve(&state, intent) else { continue };
                let before = bytes(&state);
                edit.apply(&mut state).map_err(|e| TestCaseError::fail(format!("the edit fails: {e}")))?;
                // Each step is undone by its own inverse.
                let mut undone = state.clone();
                edit.invert()
                    .unwrap()
                    .apply(&mut undone)
                    .map_err(|e| TestCaseError::fail(format!("the inverse fails: {e}")))?;
                prop_assert_eq!(bytes(&undone), before);
                applied.push(edit);
            }
            // The whole script, as one batch, is undone by its inverse.
            let script = TableEdit::Batch(applied);
            let mut replay = start.clone();
            script
                .apply(&mut replay)
                .map_err(|e| TestCaseError::fail(format!("the script fails: {e}")))?;
            prop_assert_eq!(bytes(&replay), bytes(&state));
            script
                .invert()
                .unwrap()
                .apply(&mut replay)
                .map_err(|e| TestCaseError::fail(format!("the inverse script fails: {e}")))?;
            prop_assert_eq!(bytes(&replay), bytes(&start));
            // The script followed by a step that fails leaves the start as
            // it was, every applied step rolled back.
            let failing = TableEdit::Batch(vec![
                script,
                TableEdit::RemoveForeignKey { edge: fk(), src: RowId(u32::MAX), tgt: RowId(u32::MAX) },
            ]);
            let mut refused = start.clone();
            prop_assert!(failing.apply(&mut refused).is_err());
            prop_assert_eq!(bytes(&refused), bytes(&start));
        }
    }

    /// A small sample of every surface step, valid in turn from
    /// [`sample_instance`].
    fn every_step() -> Vec<TableEdit> {
        vec![
            TableEdit::Identity,
            TableEdit::InsertRow {
                table: "post".into(),
                id: RowId(3),
                row: named("draft"),
            },
            TableEdit::AddForeignKey {
                edge: fk(),
                src: RowId(3),
                tgt: RowId(1),
            },
            TableEdit::UpdateCell {
                table: "user".into(),
                id: RowId(1),
                column: "name".into(),
                old: Some(Value::Str("bob".into())),
                new: None,
            },
            TableEdit::MoveRow {
                id: RowId(1),
                old_table: "user".into(),
                new_table: "user".into(),
            },
            TableEdit::RemoveForeignKey {
                edge: fk(),
                src: RowId(3),
                tgt: RowId(1),
            },
            TableEdit::DeleteRow {
                table: "user".into(),
                id: RowId(0),
                row: named("alice"),
                incident_fks: vec![(fk(), RowId(2), RowId(0))],
            },
        ]
    }

    #[test]
    fn table_edit_to_delta_cases() {
        let id = TableDelta::identity;
        let steps = every_step();
        let expected = [
            id(),
            id().with_added_row(RowId(3), "post", named("draft")),
            id().with_added_pair(fk(), RowId(3), RowId(1)),
            id().with_cell_change(RowId(1), "name", Some(Value::Str("bob".into())), None),
            id(),
            id().with_deleted_pair(fk(), RowId(3), RowId(1)),
            id().with_deleted_row(RowId(0), "user", named("alice"))
                .with_deleted_pair(fk(), RowId(2), RowId(0)),
        ];
        for (step, delta) in steps.iter().zip(&expected) {
            assert_eq!(&step.to_delta().unwrap(), delta, "{step:?}");
        }
        let moved = TableEdit::MoveRow {
            id: RowId(1),
            old_table: "user".into(),
            new_table: "admin".into(),
        };
        assert_eq!(
            moved.to_delta().unwrap(),
            id().with_table_change(RowId(1), "user", "admin")
        );
        let delta = id().with_table_change(RowId(1), "user", "admin");
        assert_eq!(TableEdit::Delta(delta.clone()).to_delta().unwrap(), delta);
        // A batch is the net delta of its steps: the inserted post and its
        // pair cancel against their removal only where the payloads agree.
        let batch = TableEdit::Batch(steps.clone());
        let net = expected
            .iter()
            .try_fold(id(), |acc, d| acc.compose(d))
            .unwrap();
        assert_eq!(batch.to_delta().unwrap(), net);
        assert_eq!(
            net,
            id().with_added_row(RowId(3), "post", named("draft"))
                .with_cell_change(RowId(1), "name", Some(Value::Str("bob".into())), None)
                .with_deleted_row(RowId(0), "user", named("alice"))
                .with_deleted_pair(fk(), RowId(2), RowId(0))
        );
        let mut inst = sample_instance();
        batch.apply(&mut inst).unwrap();
        let mut stepwise = sample_instance();
        for step in &steps {
            step.apply(&mut stepwise).unwrap();
        }
        assert_eq!(bytes(&inst), bytes(&stepwise));
        // Steps that do not compose are a composition error.
        let clash = TableEdit::Batch(vec![steps[1].clone(), steps[1].clone()]);
        assert_eq!(
            clash.to_delta(),
            Err(ComposeError::Mismatch {
                elem: crate::graph_edit::ElemRef::Row(RowId(3))
            })
        );
    }

    #[test]
    fn inverting_a_row_delete_reinserts_its_foreign_keys_in_order() {
        let other = Edge {
            src: "user".into(),
            tgt: "user".into(),
            kind: "boss".into(),
            name: None,
        };
        let mut inst = sample_instance();
        inst.insert_foreign_key(other.clone(), RowId(1), RowId(0))
            .unwrap();
        inst.insert_foreign_key(other, RowId(0), RowId(1)).unwrap();
        let incident = inst.incident_pairs(RowId(0));
        assert_eq!(incident.len(), 3);
        let delete = TableEdit::DeleteRow {
            table: "user".into(),
            id: RowId(0),
            row: named("alice"),
            incident_fks: incident.clone(),
        };
        let mut reinsert = vec![TableEdit::InsertRow {
            table: "user".into(),
            id: RowId(0),
            row: named("alice"),
        }];
        reinsert.extend(
            incident
                .iter()
                .map(|(edge, src, tgt)| TableEdit::AddForeignKey {
                    edge: edge.clone(),
                    src: *src,
                    tgt: *tgt,
                }),
        );
        let expected = TableEdit::Batch(reinsert).to_delta().unwrap();
        assert_eq!(delete.invert().unwrap(), TableEdit::Delta(expected));
        let before = bytes(&inst);
        delete.apply(&mut inst).unwrap();
        delete.invert().unwrap().apply(&mut inst).unwrap();
        assert_eq!(bytes(&inst), before);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn every_table_edit_lowers_to_a_normal_table_delta(
            from_empty in any::<bool>(),
            intents in proptest::collection::vec(intent(), 0..16)
        ) {
            let start = if from_empty { FInstance::new() } else { sample_instance() };
            let mut state = start.clone();
            let mut applied = Vec::new();
            for intent in &intents {
                let Some(edit) = resolve(&state, intent) else { continue };
                let delta = edit.to_delta().unwrap();
                // Normal form: no no-op change, no change of a row the delta
                // deletes or adds; re-normalizing changes nothing.
                prop_assert_eq!(TableDelta::normalize(delta.to_parts()), delta.clone());
                for ((row, _), change) in delta.cells() {
                    prop_assert!(!delta.del_rows().contains_key(row) && !delta.add_rows().contains_key(row));
                    prop_assert!(change.old.is_some() || change.new.is_some());
                }
                for change in delta.tables().values() {
                    prop_assert!(change.old != change.new);
                }
                let mut via_delta = state.clone();
                delta.apply(&mut via_delta).map_err(|e| TestCaseError::fail(format!("{e}")))?;
                edit.apply(&mut state).map_err(|e| TestCaseError::fail(format!("{e}")))?;
                prop_assert_eq!(bytes(&via_delta), bytes(&state));
                applied.push(edit);
            }
            let script = TableEdit::Batch(applied);
            let net = script.to_delta().unwrap();
            prop_assert_eq!(&TableDelta::normalize(net.to_parts()), &net);
            let mut replay = start.clone();
            net.apply(&mut replay).map_err(|e| TestCaseError::fail(format!("{e}")))?;
            prop_assert_eq!(bytes(&replay), bytes(&state));
            prop_assert_eq!(TableDelta::diff(&start, &state).apply(&mut start.clone()), Ok(()));
        }
    }
}
