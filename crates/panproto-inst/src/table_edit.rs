//! Edit algebra for functor instances (model of `ThEditableStructure`).
//!
//! A [`TableEdit`] is an element of the edit monoid for relational
//! (table-shaped) instances. Every edit names the rows it touches by
//! [`RowId`] and carries what it expects to find there (compare and set), so
//! applying it checks the instance before changing anything, and every edit
//! has an inverse ([`TableEdit::invert`]) built from its own payload. The
//! monoid operations are [`TableEdit::identity`], [`TableEdit::compose`], and
//! [`TableEdit::apply`] (the partial monoid action on [`FInstance`]).

use panproto_gat::Name;
use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use crate::edit_error::EditError;
use crate::error::InstError;
use crate::functor::{FInstance, ForeignKeyPair, Row};
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
/// rename.invert().apply(&mut tables)?;
/// insert.invert().apply(&mut tables)?;
/// assert_eq!(tables.total_row_count(), 0);
/// # Ok::<(), panproto_inst::EditError>(())
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
            _ => false,
        }
    }

    /// The edit that undoes this one, built from its compare-and-set payload.
    ///
    /// Applying an edit and then its inverse to an instance the edit applies
    /// to gives back that instance. The inverse of a `DeleteRow` re-inserts
    /// the row and then its incident pairs in ascending order.
    #[must_use]
    pub fn invert(&self) -> Self {
        match self {
            Self::Identity => Self::Identity,
            Self::InsertRow { table, id, row } => Self::DeleteRow {
                table: table.clone(),
                id: *id,
                row: row.clone(),
                incident_fks: Vec::new(),
            },
            Self::DeleteRow {
                table,
                id,
                row,
                incident_fks,
            } => {
                let mut steps = vec![Self::InsertRow {
                    table: table.clone(),
                    id: *id,
                    row: row.clone(),
                }];
                steps.extend(
                    incident_fks
                        .iter()
                        .map(|(edge, src, tgt)| Self::AddForeignKey {
                            edge: edge.clone(),
                            src: *src,
                            tgt: *tgt,
                        }),
                );
                Self::Batch(steps)
            }
            Self::UpdateCell {
                table,
                id,
                column,
                old,
                new,
            } => Self::UpdateCell {
                table: table.clone(),
                id: *id,
                column: column.clone(),
                old: new.clone(),
                new: old.clone(),
            },
            Self::MoveRow {
                id,
                old_table,
                new_table,
            } => Self::MoveRow {
                id: *id,
                old_table: new_table.clone(),
                new_table: old_table.clone(),
            },
            Self::AddForeignKey { edge, src, tgt } => Self::RemoveForeignKey {
                edge: edge.clone(),
                src: *src,
                tgt: *tgt,
            },
            Self::RemoveForeignKey { edge, src, tgt } => Self::AddForeignKey {
                edge: edge.clone(),
                src: *src,
                tgt: *tgt,
            },
            Self::Batch(steps) => Self::Batch(steps.iter().rev().map(Self::invert).collect()),
        }
    }

    /// Apply this edit to a functor instance.
    ///
    /// Every expectation the edit carries is checked against the instance
    /// before it changes; on error the instance is unchanged (a batch is
    /// applied to a copy and committed only when every step succeeds).
    ///
    /// # Errors
    ///
    /// - [`EditError::DuplicateNodeId`]: an inserted row's id is in use.
    /// - [`EditError::RowNotFound`]: the named row does not exist.
    /// - [`EditError::AnchorMismatch`]: the row is in another table than the
    ///   edit expects.
    /// - [`EditError::ApplyFailed`]: a stale expectation (the row's cells, a
    ///   cell's old value, or the incident pairs of a deleted row differ from
    ///   the instance), a pair to add is present or does not join its edge's
    ///   endpoint tables, a pair to remove is absent, or a row to move takes
    ///   part in a foreign key.
    pub fn apply(&self, instance: &mut FInstance) -> Result<(), EditError> {
        match self {
            Self::Identity => Ok(()),
            Self::InsertRow { table, id, row } => {
                if instance.table_of(*id).is_some() {
                    return Err(EditError::DuplicateNodeId(id.0));
                }
                instance
                    .insert_row(table.clone(), *id, row.clone())
                    .map_err(inst_error)
            }
            Self::DeleteRow {
                table,
                id,
                row,
                incident_fks,
            } => {
                expect_row(instance, table, *id)?;
                if instance.row(*id) != Some(row) {
                    return Err(EditError::ApplyFailed(format!(
                        "row {} does not hold the cells the deletion expects",
                        id.0
                    )));
                }
                if instance.incident_pairs(*id) != *incident_fks {
                    return Err(EditError::ApplyFailed(format!(
                        "row {} does not take part in exactly the foreign-key pairs the deletion lists",
                        id.0
                    )));
                }
                instance.remove_row(*id);
                Ok(())
            }
            Self::UpdateCell {
                table,
                id,
                column,
                old,
                new,
            } => {
                expect_row(instance, table, *id)?;
                let cells = instance
                    .row_mut(*id)
                    .ok_or_else(|| row_not_found(table, *id))?;
                if cells.get(column) != old.as_ref() {
                    return Err(EditError::ApplyFailed(format!(
                        "cell `{column}` of row {} does not hold the value the update expects",
                        id.0
                    )));
                }
                match new {
                    Some(value) => {
                        cells.insert(column.clone(), value.clone());
                    }
                    None => {
                        cells.remove(column);
                    }
                }
                Ok(())
            }
            Self::MoveRow {
                id,
                old_table,
                new_table,
            } => {
                expect_row(instance, old_table, *id)?;
                instance
                    .move_row(*id, new_table.clone())
                    .map(|_| ())
                    .map_err(inst_error)
            }
            Self::AddForeignKey { edge, src, tgt } => instance
                .insert_foreign_key(edge.clone(), *src, *tgt)
                .map_err(inst_error),
            Self::RemoveForeignKey { edge, src, tgt } => {
                if instance.remove_foreign_key(edge, *src, *tgt) {
                    Ok(())
                } else {
                    Err(EditError::ApplyFailed(format!(
                        "foreign key {edge:?} has no pair ({}, {})",
                        src.0, tgt.0
                    )))
                }
            }
            Self::Batch(steps) => {
                let mut staged = instance.clone();
                for step in steps {
                    step.apply(&mut staged)?;
                }
                *instance = staged;
                Ok(())
            }
        }
    }
}

fn row_not_found(table: &Name, id: RowId) -> EditError {
    EditError::RowNotFound {
        table: table.to_string(),
        key: format!("row {}", id.0),
    }
}

/// Check that row `id` exists and sits in `table`.
fn expect_row(instance: &FInstance, table: &Name, id: RowId) -> Result<(), EditError> {
    match instance.table_of(id) {
        None => Err(row_not_found(table, id)),
        Some(actual) if actual != table => Err(EditError::AnchorMismatch {
            node_id: id.0,
            expected: table.to_string(),
            actual: actual.to_string(),
        }),
        Some(_) => Ok(()),
    }
}

/// An instance error met while applying an edit, as an edit error.
fn inst_error(error: InstError) -> EditError {
    match error {
        InstError::DuplicateRowId { row, .. } => EditError::DuplicateNodeId(row.0),
        other => EditError::ApplyFailed(other.to_string()),
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

    fn fk() -> Edge {
        Edge {
            src: "post".into(),
            tgt: "user".into(),
            kind: "fk".into(),
            name: None,
        }
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
        assert!(matches!(
            edit.apply(&mut inst),
            Err(EditError::DuplicateNodeId(2))
        ));
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
        edit.invert().apply(&mut inst).unwrap();
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
        assert!(matches!(
            stale.apply(&mut inst),
            Err(EditError::ApplyFailed(_))
        ));
        let wrong_cells = TableEdit::DeleteRow {
            table: "user".into(),
            id: RowId(0),
            row: named("alicia"),
            incident_fks: vec![(fk(), RowId(2), RowId(0))],
        };
        assert!(matches!(
            wrong_cells.apply(&mut inst),
            Err(EditError::ApplyFailed(_))
        ));
        let wrong_table = TableEdit::DeleteRow {
            table: "post".into(),
            id: RowId(0),
            row: named("alice"),
            incident_fks: vec![(fk(), RowId(2), RowId(0))],
        };
        assert!(matches!(
            wrong_table.apply(&mut inst),
            Err(EditError::AnchorMismatch { node_id: 0, .. })
        ));
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
        assert!(matches!(
            add.apply(&mut inst),
            Err(EditError::ApplyFailed(_))
        ));
        let remove = TableEdit::UpdateCell {
            table: "user".into(),
            id: RowId(1),
            column: "name".into(),
            old: Some(Value::Str("bob".into())),
            new: None,
        };
        remove.apply(&mut inst).unwrap();
        assert!(!inst.row(RowId(1)).unwrap().contains_key("name"));
        remove.invert().apply(&mut inst).unwrap();
        add.invert().apply(&mut inst).unwrap();
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
        insert.invert().apply(&mut state).unwrap();
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
        moved.invert().apply(&mut state).unwrap();
        assert_eq!(state, start);
        assert_eq!(state.table_count(), 1);
    }

    /// The tables a generated script draws from; `tag` starts absent.
    const TABLES: [&str; 3] = ["user", "post", "tag"];

    /// One step of a generated edit script, resolved against the state it
    /// applies to.
    #[derive(Clone, Debug)]
    enum Intent {
        Insert(u8),
        Delete(u8),
        Update(u8, Option<i64>),
        Move(u8, u8),
        Link(u8, u8),
        Unlink(u8),
    }

    fn intent() -> impl Strategy<Value = Intent> {
        prop_oneof![
            any::<u8>().prop_map(Intent::Insert),
            any::<u8>().prop_map(Intent::Delete),
            (any::<u8>(), proptest::option::of(-3_i64..3)).prop_map(|(r, v)| Intent::Update(r, v)),
            (any::<u8>(), any::<u8>()).prop_map(|(r, to)| Intent::Move(r, to)),
            (any::<u8>(), any::<u8>()).prop_map(|(a, b)| Intent::Link(a, b)),
            any::<u8>().prop_map(Intent::Unlink),
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
                    new: value.map(Value::Int),
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
                let before = state.clone();
                edit.apply(&mut state).map_err(|e| TestCaseError::fail(format!("the edit fails: {e}")))?;
                // Each step is undone by its own inverse.
                let mut undone = state.clone();
                edit.invert()
                    .apply(&mut undone)
                    .map_err(|e| TestCaseError::fail(format!("the inverse fails: {e}")))?;
                prop_assert_eq!(&undone, &before);
                applied.push(edit);
            }
            // The whole script, as one batch, is undone by its inverse.
            let script = TableEdit::Batch(applied);
            let mut replay = start.clone();
            script
                .apply(&mut replay)
                .map_err(|e| TestCaseError::fail(format!("the script fails: {e}")))?;
            prop_assert_eq!(&replay, &state);
            script
                .invert()
                .apply(&mut replay)
                .map_err(|e| TestCaseError::fail(format!("the inverse script fails: {e}")))?;
            prop_assert_eq!(&replay, &start);
        }
    }
}
