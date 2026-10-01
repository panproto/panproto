//! Constraint-semantics rows for the database protocols other than `sql`,
//! whose rows are in `sql.rs` (`mongodb`, `dynamodb`, `cassandra`, `neo4j`).
//!
//! The `mongodb` `$jsonSchema` keywords are JSON Schema's (lengths in code
//! points, inclusive bounds); `bsonType` is the vertex kind itself and
//! `description` describes. A `cassandra` `NOT NULL` column constrains each
//! row's cell, like SQL's. Keys (`cassandra`'s `PRIMARY KEY` and clustering
//! order, `dynamodb`'s `key-type`, `neo4j`'s `unique` and `node-key`) are schema
//! declarations, checked from those declarations, and presence flags
//! (`mongodb`'s `required`, `neo4j`'s `exists`) are about the parent
//! element, which must have the property (the schema's required edge), not
//! about the property's own value; they describe here, as do `dynamodb`'s
//! provisioning settings, `cassandra`'s `STATIC` and `neo4j`'s property
//! `type`.

use panproto_schema::{ConstraintSemantics, LengthUnit};

use super::ConstraintRow;

/// The row giving sort `sort` of protocol `protocol` the meaning `semantics`
/// builds.
const fn row(
    protocol: &'static str,
    sort: &'static str,
    semantics: fn() -> ConstraintSemantics,
) -> ConstraintRow {
    ConstraintRow {
        protocol,
        sort,
        semantics,
    }
}

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("mongodb", "required", || ConstraintSemantics::Annotation),
    row("mongodb", "bsonType", || ConstraintSemantics::Annotation),
    row("mongodb", "enum", || ConstraintSemantics::Enum),
    row("mongodb", "minimum", || ConstraintSemantics::Minimum),
    row("mongodb", "maximum", || ConstraintSemantics::Maximum),
    row("mongodb", "minLength", || {
        ConstraintSemantics::MinLength(LengthUnit::Chars)
    }),
    row("mongodb", "maxLength", || {
        ConstraintSemantics::MaxLength(LengthUnit::Chars)
    }),
    row("mongodb", "pattern", || ConstraintSemantics::Pattern),
    row("mongodb", "description", || ConstraintSemantics::Annotation),
    row("dynamodb", "key-type", || ConstraintSemantics::Annotation),
    row("dynamodb", "projection-type", || {
        ConstraintSemantics::Annotation
    }),
    row("dynamodb", "read-capacity", || {
        ConstraintSemantics::Annotation
    }),
    row("dynamodb", "write-capacity", || {
        ConstraintSemantics::Annotation
    }),
    row("cassandra", "PRIMARY KEY", || {
        ConstraintSemantics::Annotation
    }),
    row("cassandra", "CLUSTERING ORDER", || {
        ConstraintSemantics::Annotation
    }),
    row("cassandra", "STATIC", || ConstraintSemantics::Annotation),
    row("cassandra", "NOT NULL", || {
        ConstraintSemantics::RequiredValue
    }),
    row("neo4j", "unique", || ConstraintSemantics::Annotation),
    row("neo4j", "exists", || ConstraintSemantics::Annotation),
    row("neo4j", "node-key", || ConstraintSemantics::Annotation),
    row("neo4j", "type", || ConstraintSemantics::Annotation),
];
