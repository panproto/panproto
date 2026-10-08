//! Complete finite scopes for the Kan-extension page.

use std::collections::BTreeMap;

use crate::migration::Migration;
use crate::model::Table;
use crate::schema::Schema;
use crate::value::Value;

/// Every two-object schema with at most one labelled edge.
#[must_use]
pub fn schemas() -> Vec<Schema> {
    super::schemas(2, &["k"], &["e"], 1)
}

/// Every vertex-only migration between the first and last bounded schemas.
#[must_use]
pub fn migrations() -> Vec<Migration> {
    let schemas = schemas();
    super::migrations(
        &schemas[0],
        &schemas[schemas.len() - 1],
        super::MigrationScope::VERTICES,
    )
}

/// Labelled one-table instances with at most one row.
#[must_use]
pub fn tables() -> Vec<Table> {
    super::tables(
        &["t"],
        &BTreeMap::from([("t", vec!["c"])]),
        &[Value::Int(0)],
        &[],
        1,
    )
}
