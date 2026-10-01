//! Constraint-semantics rows for the database protocols other than `sql`,
//! whose rows are in `sql.rs` (`mongodb`, `dynamodb`, `cassandra`, `neo4j`).
//!
//! The `mongodb` `$jsonSchema` keywords are JSON Schema's (lengths in code
//! points, inclusive bounds); `bsonType` is the vertex kind itself and
//! `description` describes. A `cassandra` `NOT NULL` column constrains each
//! row's cell, like SQL's. Keys (`cassandra`'s `PRIMARY KEY` and clustering
//! order, `dynamodb`'s `key-type`, `neo4j`'s `unique` and `node-key`) are schema
//! declarations, and presence flags (`mongodb`'s `required`, `neo4j`'s
//! `exists`) are about the parent element (see the [`common`](super::common)
//! module); they describe here, as do `dynamodb`'s provisioning settings,
//! `cassandra`'s `STATIC` and `neo4j`'s property `type`.

use super::ConstraintRow;
use super::common::{
    annotation, enumeration, max_chars, maximum, min_chars, minimum, pattern, required_value, row,
};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("mongodb", "required", annotation),
    row("mongodb", "bsonType", annotation),
    row("mongodb", "enum", enumeration),
    row("mongodb", "minimum", minimum),
    row("mongodb", "maximum", maximum),
    row("mongodb", "minLength", min_chars),
    row("mongodb", "maxLength", max_chars),
    row("mongodb", "pattern", pattern),
    row("mongodb", "description", annotation),
    row("dynamodb", "key-type", annotation),
    row("dynamodb", "projection-type", annotation),
    row("dynamodb", "read-capacity", annotation),
    row("dynamodb", "write-capacity", annotation),
    row("cassandra", "PRIMARY KEY", annotation),
    row("cassandra", "CLUSTERING ORDER", annotation),
    row("cassandra", "STATIC", annotation),
    row("cassandra", "NOT NULL", required_value),
    row("neo4j", "unique", annotation),
    row("neo4j", "exists", annotation),
    row("neo4j", "node-key", annotation),
    row("neo4j", "type", annotation),
];
