//! Content-addressed objects stored in the VCS.
//!
//! The object store contains four kinds of objects:
//! - [`Object::Schema`] — a schema snapshot
//! - [`Object::Migration`] — a morphism between two schemas
//! - [`Object::Commit`] — a point in the schema evolution DAG
//! - [`Object::Tag`] — an annotated tag pointing to another object

use panproto_gat::SiteRename;
use panproto_mig::Migration;
use panproto_schema::Schema;
use serde::{Deserialize, Serialize};

use crate::ObjectId;

/// A content-addressed object in the store.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Object {
    /// A schema snapshot.
    Schema(Box<Schema>),

    /// A migration between two schemas, identified by their object IDs.
    Migration {
        /// Object ID of the source schema.
        src: ObjectId,
        /// Object ID of the target schema.
        tgt: ObjectId,
        /// The migration morphism.
        mapping: Migration,
    },

    /// A commit in the schema evolution DAG.
    Commit(CommitObject),

    /// An annotated tag pointing to another object.
    Tag(TagObject),
}

impl Object {
    /// Returns the type name of this object (for error messages).
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Schema(_) => "schema",
            Self::Migration { .. } => "migration",
            Self::Commit(_) => "commit",
            Self::Tag(_) => "tag",
        }
    }
}

/// A commit in the schema evolution DAG.
///
/// Commits form a DAG via parent pointers. A root commit has no parents,
/// a normal commit has one parent, and a merge commit has two.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitObject {
    /// Object ID of the schema at this commit.
    pub schema_id: ObjectId,

    /// Parent commit IDs (0 = root, 1 = normal, 2 = merge).
    pub parents: Vec<ObjectId>,

    /// Object ID of the migration from parent's schema to this commit's
    /// schema. `None` for root commits.
    pub migration_id: Option<ObjectId>,

    /// The protocol this schema lineage tracks.
    pub protocol: String,

    /// Author identifier.
    pub author: String,

    /// Unix timestamp in seconds.
    pub timestamp: u64,

    /// Human-readable commit message.
    pub message: String,

    /// Renames detected or declared for this commit's migration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub renames: Vec<SiteRename>,
}

/// An annotated tag object.
///
/// Unlike lightweight tags (which are just refs pointing directly at a
/// commit), annotated tags are stored as objects in the store and carry
/// metadata: tagger, timestamp, and message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TagObject {
    /// Object ID of the tagged object (usually a commit).
    pub target: ObjectId,

    /// Who created the tag.
    pub tagger: String,

    /// Unix timestamp in seconds.
    pub timestamp: u64,

    /// Tag message.
    pub message: String,
}
