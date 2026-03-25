//! Edit algebra for functor instances (model of `ThEditableStructure`).
//!
//! A [`TableEdit`] is an element of the edit monoid for relational
//! (table-shaped) instances. The monoid operations are
//! [`TableEdit::identity`], [`TableEdit::compose`], and
//! [`TableEdit::apply`] (the partial monoid action on [`FInstance`]).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::edit_error::EditError;
use crate::functor::FInstance;
use crate::value::Value;

/// A model of `ThEditableStructure` for functor instances.
///
/// Each variant is a primitive relational mutation: inserting or
/// deleting rows, or updating individual cells.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TableEdit {
    /// The monoid identity: no change.
    Identity,

    /// Insert a row into a table.
    InsertRow {
        /// Table name (schema vertex ID).
        table: String,
        /// The row data (column name to value).
        row: HashMap<String, Value>,
    },

    /// Delete a row from a table by key column value.
    DeleteRow {
        /// Table name.
        table: String,
        /// The key value identifying the row to delete.
        key: Value,
        /// The key column name.
        key_column: String,
    },

    /// Update a single cell in a table.
    UpdateCell {
        /// Table name.
        table: String,
        /// Key value identifying the row.
        key: Value,
        /// Key column name.
        key_column: String,
        /// Column to update.
        column: String,
        /// New value.
        value: Value,
    },

    /// A sequence of edits applied in order.
    Sequence(Vec<Self>),
}

impl TableEdit {
    /// The monoid identity element.
    #[must_use]
    pub const fn identity() -> Self {
        Self::Identity
    }

    /// Monoid multiplication: compose two edits into a sequence.
    ///
    /// Nested sequences are flattened and identity elements are elided.
    #[must_use]
    pub fn compose(self, other: Self) -> Self {
        let mut steps = Vec::new();
        flatten_into(&mut steps, self);
        flatten_into(&mut steps, other);
        match steps.len() {
            0 => Self::Identity,
            1 => steps.into_iter().next().unwrap_or(Self::Identity),
            _ => Self::Sequence(steps),
        }
    }

    /// Returns `true` if this edit is the identity (no-op).
    #[must_use]
    pub fn is_identity(&self) -> bool {
        match self {
            Self::Identity => true,
            Self::Sequence(steps) => steps.iter().all(Self::is_identity),
            _ => false,
        }
    }

    /// Apply this edit to a functor instance, mutating it in place.
    ///
    /// # Errors
    ///
    /// Returns [`EditError`] if the edit cannot be applied.
    pub fn apply(&self, instance: &mut FInstance) -> Result<(), EditError> {
        match self {
            Self::Identity => Ok(()),

            Self::InsertRow { table, row } => {
                instance
                    .tables
                    .entry(table.clone())
                    .or_default()
                    .push(row.clone());
                Ok(())
            }

            Self::DeleteRow {
                table,
                key,
                key_column,
            } => {
                let rows = instance
                    .tables
                    .get_mut(table.as_str())
                    .ok_or_else(|| EditError::TableNotFound(table.clone()))?;
                let before = rows.len();
                rows.retain(|row| row.get(key_column.as_str()) != Some(key));
                if rows.len() == before {
                    return Err(EditError::RowNotFound {
                        table: table.clone(),
                        key: format!("{key:?}"),
                    });
                }
                Ok(())
            }

            Self::UpdateCell {
                table,
                key,
                key_column,
                column,
                value,
            } => {
                let rows = instance
                    .tables
                    .get_mut(table.as_str())
                    .ok_or_else(|| EditError::TableNotFound(table.clone()))?;
                let row = rows
                    .iter_mut()
                    .find(|r| r.get(key_column.as_str()) == Some(key))
                    .ok_or_else(|| EditError::RowNotFound {
                        table: table.clone(),
                        key: format!("{key:?}"),
                    })?;
                row.insert(column.clone(), value.clone());
                Ok(())
            }

            Self::Sequence(steps) => {
                for step in steps {
                    step.apply(instance)?;
                }
                Ok(())
            }
        }
    }
}

/// Flatten nested sequences and strip identities.
fn flatten_into(out: &mut Vec<TableEdit>, edit: TableEdit) {
    match edit {
        TableEdit::Identity => {}
        TableEdit::Sequence(steps) => {
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

    use crate::functor::FInstance;
    use crate::value::Value;

    use super::TableEdit;

    fn sample_instance() -> FInstance {
        let mut row1 = HashMap::new();
        row1.insert("id".into(), Value::Int(1));
        row1.insert("name".into(), Value::Str("alice".into()));

        let mut row2 = HashMap::new();
        row2.insert("id".into(), Value::Int(2));
        row2.insert("name".into(), Value::Str("bob".into()));

        FInstance::new().with_table("users", vec![row1, row2])
    }

    #[test]
    fn identity_is_noop() {
        let mut inst = sample_instance();
        TableEdit::identity().apply(&mut inst).unwrap();
        assert_eq!(inst.row_count("users"), 2);
    }

    #[test]
    fn insert_row() {
        let mut inst = sample_instance();
        let mut row = HashMap::new();
        row.insert("id".into(), Value::Int(3));
        row.insert("name".into(), Value::Str("charlie".into()));

        let edit = TableEdit::InsertRow {
            table: "users".into(),
            row,
        };
        edit.apply(&mut inst).unwrap();
        assert_eq!(inst.row_count("users"), 3);
    }

    #[test]
    fn delete_row() {
        let mut inst = sample_instance();
        let edit = TableEdit::DeleteRow {
            table: "users".into(),
            key: Value::Int(1),
            key_column: "id".into(),
        };
        edit.apply(&mut inst).unwrap();
        assert_eq!(inst.row_count("users"), 1);
    }

    #[test]
    fn update_cell() {
        let mut inst = sample_instance();
        let edit = TableEdit::UpdateCell {
            table: "users".into(),
            key: Value::Int(1),
            key_column: "id".into(),
            column: "name".into(),
            value: Value::Str("alicia".into()),
        };
        edit.apply(&mut inst).unwrap();
        let rows = &inst.tables["users"];
        let row = rows
            .iter()
            .find(|r| r.get("id") == Some(&Value::Int(1)))
            .unwrap();
        assert_eq!(row.get("name"), Some(&Value::Str("alicia".into())));
    }

    #[test]
    fn insert_then_delete_is_identity() {
        let mut inst = sample_instance();
        let original_count = inst.row_count("users");

        let mut row = HashMap::new();
        row.insert("id".into(), Value::Int(99));
        row.insert("name".into(), Value::Str("temp".into()));

        let edit = TableEdit::InsertRow {
            table: "users".into(),
            row,
        }
        .compose(TableEdit::DeleteRow {
            table: "users".into(),
            key: Value::Int(99),
            key_column: "id".into(),
        });
        edit.apply(&mut inst).unwrap();
        assert_eq!(inst.row_count("users"), original_count);
    }

    #[test]
    fn delete_from_nonexistent_table_fails() {
        let mut inst = sample_instance();
        let edit = TableEdit::DeleteRow {
            table: "nonexistent".into(),
            key: Value::Int(1),
            key_column: "id".into(),
        };
        assert!(edit.apply(&mut inst).is_err());
    }

    #[test]
    fn monoid_identity_law() {
        let mut inst1 = sample_instance();
        let mut inst2 = sample_instance();

        let mut row = HashMap::new();
        row.insert("id".into(), Value::Int(5));
        row.insert("name".into(), Value::Str("eve".into()));

        let edit = TableEdit::InsertRow {
            table: "users".into(),
            row: row.clone(),
        };

        TableEdit::identity()
            .compose(edit.clone())
            .apply(&mut inst1)
            .unwrap();
        edit.apply(&mut inst2).unwrap();

        assert_eq!(inst1.row_count("users"), inst2.row_count("users"));
    }
}
