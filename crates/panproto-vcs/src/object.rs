//! Content-addressed objects stored in the VCS.
//!
//! The object store contains seven kinds of objects:
//! - [`Object::Schema`] — a schema snapshot
//! - [`Object::Migration`] — a morphism between two schemas
//! - [`Object::Commit`] — a point in the schema evolution DAG
//! - [`Object::Tag`] — an annotated tag pointing to another object
//! - [`Object::DataSet`] — a data snapshot conforming to a schema
//! - [`Object::Complement`] — a complement from data migration
//! - [`Object::Protocol`] — a protocol (metaschema) definition

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

    /// A data snapshot — instances conforming to a specific schema.
    DataSet(DataSetObject),

    /// A complement from data migration, for backward migration.
    Complement(ComplementObject),

    /// A protocol (metaschema) definition.
    Protocol(Box<panproto_schema::Protocol>),
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
            Self::DataSet(_) => "dataset",
            Self::Complement(_) => "complement",
            Self::Protocol(_) => "protocol",
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
    #[serde(default)]
    pub renames: Vec<SiteRename>,

    /// Object ID of the protocol definition at this commit.
    #[serde(default)]
    pub protocol_id: Option<ObjectId>,

    /// Object IDs of data sets tracked at this commit.
    #[serde(default)]
    pub data_ids: Vec<ObjectId>,

    /// Object IDs of complements from the migration at this commit.
    #[serde(default)]
    pub complement_ids: Vec<ObjectId>,
}

/// A data snapshot stored in the VCS.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataSetObject {
    /// Which schema this data conforms to.
    pub schema_id: ObjectId,
    /// MessagePack-encoded instance data.
    pub data: Vec<u8>,
    /// Number of records.
    pub record_count: u64,
}

/// A complement from data migration, enabling backward migration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComplementObject {
    /// The migration that produced this complement.
    pub migration_id: ObjectId,
    /// The data set this complement was computed from.
    pub data_id: ObjectId,
    /// MessagePack-encoded Complement data.
    pub complement: Vec<u8>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dataset_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let ds = DataSetObject {
            schema_id: ObjectId::ZERO,
            data: vec![1, 2, 3, 4],
            record_count: 42,
        };
        let bytes = rmp_serde::to_vec(&ds)?;
        let ds2: DataSetObject = rmp_serde::from_slice(&bytes)?;
        assert_eq!(ds.schema_id, ds2.schema_id);
        assert_eq!(ds.data, ds2.data);
        assert_eq!(ds.record_count, ds2.record_count);
        Ok(())
    }

    #[test]
    fn complement_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let comp = ComplementObject {
            migration_id: ObjectId::from_bytes([1; 32]),
            data_id: ObjectId::from_bytes([2; 32]),
            complement: vec![10, 20, 30],
        };
        let bytes = rmp_serde::to_vec(&comp)?;
        let comp2: ComplementObject = rmp_serde::from_slice(&bytes)?;
        assert_eq!(comp.migration_id, comp2.migration_id);
        assert_eq!(comp.data_id, comp2.data_id);
        assert_eq!(comp.complement, comp2.complement);
        Ok(())
    }
}
