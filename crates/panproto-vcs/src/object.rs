//! Content-addressed objects stored in the VCS.
//!
//! The object store contains the following kinds of objects:
//! - [`Object::FileSchema`]: a per-file schema leaf
//! - [`Object::SchemaTree`]: an inner node of the project schema Merkle tree
//! - [`Object::Migration`]: a morphism between two schemas
//! - [`Object::Commit`]: a point in the schema evolution DAG
//! - [`Object::Tag`]: an annotated tag pointing to another object
//! - [`Object::DataSet`]: an instance (tree, table or graph) conforming to a
//!   schema
//! - [`Object::Complement`]: a complement from data migration
//! - [`Object::Protocol`]: a protocol (metaschema) definition
//! - [`Object::Expr`]: a standalone expression (coercion, merge, default)
//! - [`Object::EditLog`]: an edit log (tree edits, table deltas or graph
//!   deltas) for incremental migration
//! - [`Object::Theory`]: a GAT theory definition
//! - [`Object::TheoryMorphism`]: a structure-preserving map between theories
//!
//! The payloads of [`Object::DataSet`], [`Object::Complement`] and
//! [`Object::EditLog`] are typed by a [`PayloadEncoding`]: the shape of
//! instance they belong to and the version of that shape's encoding.
//! [`crate::payload`] encodes and decodes them.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::SiteRename;
use panproto_mig::Migration;
use panproto_schema::Schema;
use serde::{Deserialize, Serialize};

use crate::ObjectId;

/// A content-addressed object in the store.
///
/// Marked `#[non_exhaustive]` so that adding new variants downstream
/// (new on-disk object kinds, new enrichments) does not silently break
/// exhaustive `match` expressions in consumers of this crate.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Object {
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

    /// A data snapshot: instances conforming to a specific schema.
    DataSet(DataSetObject),

    /// A complement from data migration, for backward migration.
    Complement(ComplementObject),

    /// A protocol (metaschema) definition.
    Protocol(Box<panproto_schema::Protocol>),

    /// A standalone expression (e.g., coercion, merge, default).
    Expr(Box<panproto_expr::Expr>),

    /// An edit log recording incremental edits against a schema.
    EditLog(EditLogObject),

    /// A GAT theory definition.
    Theory(Box<panproto_gat::Theory>),

    /// A structure-preserving map between two theories.
    TheoryMorphism(Box<panproto_gat::TheoryMorphism>),

    /// A CST complement for format-preserving round-trips.
    ///
    /// Stores the full tree-sitter CST Schema alongside a data set,
    /// enabling byte-identical reconstruction of the original file
    /// formatting after schema migration.
    CstComplement(CstComplementObject),

    /// A schema for a single file.
    ///
    /// Content-addressed independently so that unchanged files
    /// deduplicate across commits. The project-level schema is
    /// assembled by walking a tree of [`Self::FileSchema`] leaves
    /// joined by [`Self::SchemaTree`] nodes; see [`crate::tree`].
    FileSchema(Box<FileSchemaObject>),

    /// A flat schema, stored only as a migration endpoint.
    ///
    /// [`Object::Migration`] references its source and target schemas
    /// by [`ObjectId`]. Commits content-address their project schema
    /// as an [`Object::SchemaTree`] whose leaves are
    /// [`Object::FileSchema`]s, so a migration between two tree
    /// roots cannot reuse that addressing: the migration morphism
    /// speaks of a single flat vertex/edge space, not a tree. This
    /// variant stores that flat form so that `gc::mark_reachable`
    /// can find the referenced schema and migration composition has
    /// an object to load from.
    FlatSchema(Box<panproto_schema::Schema>),

    /// A directory or project root in the schema Merkle tree.
    ///
    /// A sorted list of `(name, ObjectId)` entries where each
    /// [`ObjectId`] points at either an [`Self::FileSchema`] leaf or
    /// another [`Self::SchemaTree`]. Mirrors git's tree object
    /// model: sibling entries are sorted lexicographically so the
    /// resulting [`ObjectId`] is deterministic.
    SchemaTree(Box<SchemaTreeObject>),
}

impl Object {
    /// Returns the type name of this object (for error messages).
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Migration { .. } => "migration",
            Self::Commit(_) => "commit",
            Self::Tag(_) => "tag",
            Self::DataSet(_) => "dataset",
            Self::Complement(_) => "complement",
            Self::Protocol(_) => "protocol",
            Self::Expr(_) => "expr",
            Self::EditLog(_) => "editlog",
            Self::Theory(_) => "theory",
            Self::TheoryMorphism(_) => "theory_morphism",
            Self::CstComplement(_) => "cst_complement",
            Self::FileSchema(_) => "file_schema",
            Self::SchemaTree(_) => "schema_tree",
            Self::FlatSchema(_) => "flat_schema",
        }
    }
}

/// A per-file schema leaf in the project schema Merkle tree.
///
/// Carries the full parsed [`Schema`] for one file alongside the
/// file's path and the protocol used to parse it. Stored as an
/// [`Object::FileSchema`] so its [`ObjectId`] depends only on the
/// per-file content.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileSchemaObject {
    /// Path of the file within the project, using forward slashes.
    pub path: String,
    /// Protocol used to parse this file (e.g., `"typescript"`,
    /// `"raw_file"`).
    pub protocol: String,
    /// The per-file schema as produced by the protocol's parser.
    pub schema: Schema,
    /// Cross-file import edges rooted at a vertex in this file.
    ///
    /// Each edge carries vertex names already prefixed with the
    /// owning file's project path (`<src_path>::<src_name>` and
    /// `<tgt_path>::<tgt_name>`), so flat assembly can add them
    /// without re-prefixing. Populated by
    /// `panproto_project::ProjectBuilder::build_tree`; empty
    /// otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cross_file_edges: Vec<panproto_schema::Edge>,
}

/// An entry in a [`SchemaTreeObject::Directory`] node.
///
/// Distinguishes leaf (`File`, pointing at a [`FileSchemaObject`])
/// from inner (`Tree`, pointing at another [`SchemaTreeObject`]) so
/// the tree walker does not need to re-fetch objects just to
/// classify them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SchemaTreeEntry {
    /// A file-schema leaf named by its parent tree.
    File(ObjectId),
    /// A subtree named by its parent tree.
    Tree(ObjectId),
}

/// An inner node of the project schema Merkle tree.
///
/// A [`SchemaTreeObject::Directory`] holds named entries sorted
/// lexicographically by name so two directories with the same
/// `(name, entry)` set hash to the same [`ObjectId`] regardless of
/// how they were constructed. A [`SchemaTreeObject::SingleLeaf`] is a
/// nameless wrapper over a single [`FileSchemaObject`], produced by
/// [`crate::tree::store_schema_as_tree`] when a caller needs to store
/// a flat [`panproto_schema::Schema`] under the tree-based commit
/// model. Keeping the two shapes as distinct variants of the object
/// itself (rather than a name slot alongside a `SingleLeaf` entry)
/// means a peer cannot forge a `(name="x", SingleLeaf(id))` entry
/// that walks identically to the canonical form but hashes
/// differently.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SchemaTreeObject {
    /// Nameless wrapper around a single file schema.
    SingleLeaf {
        /// [`ObjectId`] of the wrapped [`FileSchemaObject`].
        file_schema_id: ObjectId,
    },
    /// Directory node holding named child entries.
    Directory {
        /// Child entries, canonically sorted lexicographically by name.
        ///
        /// Callers consuming a deserialized `Directory` should iterate
        /// via [`SchemaTreeObject::sorted_entries`] to enforce the
        /// canonical ordering regardless of how the bytes arrived on
        /// the wire.
        entries: Vec<(String, SchemaTreeEntry)>,
    },
}

impl SchemaTreeObject {
    /// Return a canonically ordered view of a directory's entries,
    /// or an empty vector for a [`SchemaTreeObject::SingleLeaf`].
    ///
    /// Entries are sorted lexicographically by name. Use this anywhere
    /// the flat-schema hash or walk order is observable, so a remote
    /// cannot produce different flat hashes for the same tree
    /// [`ObjectId`] by permuting wire-order.
    #[must_use]
    pub fn sorted_entries(&self) -> Vec<(&str, &SchemaTreeEntry)> {
        match self {
            Self::SingleLeaf { .. } => Vec::new(),
            Self::Directory { entries } => {
                let mut out: Vec<(&str, &SchemaTreeEntry)> =
                    entries.iter().map(|(n, e)| (n.as_str(), e)).collect();
                out.sort_by(|a, b| a.0.cmp(b.0));
                out
            }
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
    ///
    /// This always points at an [`Object::SchemaTree`] root whose
    /// leaves are [`Object::FileSchema`] objects. Callers that need
    /// a flat [`Schema`] should go through
    /// [`resolve_commit_schema`](crate::resolve_commit_schema) (or
    /// [`assemble_schema`](crate::assemble_schema) when they already
    /// hold the tree id), which walks the tree and assembles the
    /// project-coproduct schema.
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
    pub renames: Vec<SiteRename>,

    /// Object ID of the protocol definition at this commit.
    pub protocol_id: Option<ObjectId>,

    /// Object IDs of data sets tracked at this commit.
    pub data_ids: Vec<ObjectId>,

    /// Object IDs of complements from the migration at this commit.
    pub complement_ids: Vec<ObjectId>,

    /// Object IDs of edit logs for incremental migration at this commit.
    pub edit_log_ids: Vec<ObjectId>,

    /// Theory object IDs at this commit, keyed by theory name.
    pub theory_ids: BTreeMap<String, ObjectId>,

    /// Object IDs of CST complements for format-preserving round-trips.
    ///
    /// Always written, even when empty. An object is stored as
    /// `MessagePack`, which writes a struct as an array of its fields in
    /// declaration order, so omitting a field does not mark it absent:
    /// it shifts every later field one slot left and the decoder reads
    /// each of them as the wrong thing, and `unverified` follows this
    /// field.
    #[serde(default)]
    pub cst_complement_ids: Vec<ObjectId>,

    /// What this commit contains that was never verified, when it was
    /// made with verification bypassed. Empty for an ordinary commit.
    ///
    /// A commit is the durable record, so the distinction between
    /// material that passed its checks and material that was waved
    /// through has to survive in it. Without this an explicit
    /// `--skip-verify` commit is indistinguishable afterwards from one
    /// whose contents were checked and passed.
    ///
    /// Read back as empty for any commit written before this field
    /// existed, which is correct in the only sense available: such a
    /// commit recorded no bypass because it had no way to.
    #[serde(default)]
    pub unverified: Vec<String>,

    /// The keys this commit stops tracking, ascending: from this commit
    /// on along first-parent history, no instance is tracked under them
    /// until a later commit records a data set under the key again
    /// ([`instance_vc::tracked`](crate::instance_vc::tracked)).
    ///
    /// The field is last and written only when nonempty, so a commit that
    /// stops tracking nothing has the bytes, and the identifier, it had
    /// before the field existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub untracked: Vec<String>,
}

impl CommitObject {
    /// Create a builder for `CommitObject` with required fields.
    #[must_use]
    pub fn builder(
        schema_id: ObjectId,
        protocol: impl Into<String>,
        author: impl Into<String>,
        message: impl Into<String>,
    ) -> CommitObjectBuilder {
        CommitObjectBuilder {
            schema_id,
            parents: Vec::new(),
            migration_id: None,
            protocol: protocol.into(),
            author: author.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            message: message.into(),
            renames: Vec::new(),
            protocol_id: None,
            data_ids: Vec::new(),
            complement_ids: Vec::new(),
            edit_log_ids: Vec::new(),
            theory_ids: BTreeMap::new(),
            cst_complement_ids: Vec::new(),
            unverified: Vec::new(),
            untracked: Vec::new(),
        }
    }
}

/// Builder for [`CommitObject`] with sensible defaults for optional fields.
pub struct CommitObjectBuilder {
    schema_id: ObjectId,
    parents: Vec<ObjectId>,
    migration_id: Option<ObjectId>,
    protocol: String,
    author: String,
    timestamp: u64,
    message: String,
    renames: Vec<SiteRename>,
    protocol_id: Option<ObjectId>,
    data_ids: Vec<ObjectId>,
    complement_ids: Vec<ObjectId>,
    edit_log_ids: Vec<ObjectId>,
    theory_ids: BTreeMap<String, ObjectId>,
    cst_complement_ids: Vec<ObjectId>,
    unverified: Vec<String>,
    untracked: Vec<String>,
}

impl CommitObjectBuilder {
    /// Set the parent commit IDs.
    #[must_use]
    pub fn parents(mut self, parents: Vec<ObjectId>) -> Self {
        self.parents = parents;
        self
    }

    /// Set the migration object ID.
    #[must_use]
    pub const fn migration_id(mut self, id: ObjectId) -> Self {
        self.migration_id = Some(id);
        self
    }

    /// Set the unix timestamp (seconds).
    #[must_use]
    pub const fn timestamp(mut self, ts: u64) -> Self {
        self.timestamp = ts;
        self
    }

    /// Set the detected/declared renames.
    #[must_use]
    pub fn renames(mut self, renames: Vec<SiteRename>) -> Self {
        self.renames = renames;
        self
    }

    /// Set the protocol definition object ID.
    #[must_use]
    pub const fn protocol_id(mut self, id: ObjectId) -> Self {
        self.protocol_id = Some(id);
        self
    }

    /// Set the data set object IDs.
    #[must_use]
    pub fn data_ids(mut self, ids: Vec<ObjectId>) -> Self {
        self.data_ids = ids;
        self
    }

    /// Set the complement object IDs.
    #[must_use]
    pub fn complement_ids(mut self, ids: Vec<ObjectId>) -> Self {
        self.complement_ids = ids;
        self
    }

    /// Set the edit log object IDs.
    #[must_use]
    pub fn edit_log_ids(mut self, ids: Vec<ObjectId>) -> Self {
        self.edit_log_ids = ids;
        self
    }

    /// Set the theory object IDs.
    #[must_use]
    pub fn theory_ids(mut self, ids: BTreeMap<String, ObjectId>) -> Self {
        self.theory_ids = ids;
        self
    }

    /// Build the [`CommitObject`].
    #[must_use]
    pub fn build(self) -> CommitObject {
        CommitObject {
            schema_id: self.schema_id,
            parents: self.parents,
            migration_id: self.migration_id,
            protocol: self.protocol,
            author: self.author,
            timestamp: self.timestamp,
            message: self.message,
            renames: self.renames,
            protocol_id: self.protocol_id,
            data_ids: self.data_ids,
            complement_ids: self.complement_ids,
            edit_log_ids: self.edit_log_ids,
            theory_ids: self.theory_ids,
            cst_complement_ids: self.cst_complement_ids,
            unverified: self.unverified,
            untracked: self.untracked,
        }
    }

    /// Set the CST complement IDs.
    #[must_use]
    pub fn cst_complement_ids(mut self, ids: Vec<ObjectId>) -> Self {
        self.cst_complement_ids = ids;
        self
    }

    /// Record what this commit contains that was never verified.
    #[must_use]
    pub fn unverified(mut self, what: Vec<String>) -> Self {
        self.unverified = what;
        self
    }

    /// Record the keys this commit stops tracking; they are stored sorted
    /// and without repetition.
    #[must_use]
    pub fn untracked(mut self, keys: Vec<String>) -> Self {
        let keys: BTreeSet<String> = keys.into_iter().collect();
        self.untracked = keys.into_iter().collect();
        self
    }
}

/// The shape of an instance: the three instance forms panproto has.
///
/// Serialized by the lowercase names `"tree"`, `"table"` and `"graph"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceShape {
    /// Tree instances ([`panproto_inst::WInstance`]).
    Tree,
    /// Table instances ([`panproto_inst::FInstance`]).
    Table,
    /// Graph instances ([`panproto_inst::GInstance`]).
    Graph,
}

impl InstanceShape {
    /// Every shape, in declaration order.
    pub const ALL: [Self; 3] = [Self::Tree, Self::Table, Self::Graph];

    /// The lowercase name the shape is serialized by.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Table => "table",
            Self::Graph => "graph",
        }
    }
}

/// How the payload of a data set, complement or edit log is encoded: the
/// shape of instance it belongs to and the version of that shape's
/// encoding.
///
/// Version 1 is the only version of each shape this build reads or
/// writes; [`crate::payload`] states each encoding and refuses every other
/// version with [`VcsError::UnsupportedPayloadEncoding`]. The tree encoding
/// of version 1 is the default: an object that records no encoding holds a
/// tree payload of version 1, and such an object is written without the
/// field, so that its bytes and its identifier are those it has always had.
///
/// [`VcsError::UnsupportedPayloadEncoding`]: crate::VcsError::UnsupportedPayloadEncoding
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PayloadEncoding {
    /// The shape of instance the payload belongs to.
    pub shape: InstanceShape,
    /// The version of the shape's encoding.
    pub version: u32,
}

impl PayloadEncoding {
    /// Version 1 of the tree encoding.
    pub const TREE: Self = Self::current(InstanceShape::Tree);
    /// Version 1 of the table encoding.
    pub const TABLE: Self = Self::current(InstanceShape::Table);
    /// Version 1 of the graph encoding.
    pub const GRAPH: Self = Self::current(InstanceShape::Graph);

    /// The encoding of `shape` this build writes.
    #[must_use]
    pub const fn current(shape: InstanceShape) -> Self {
        Self { shape, version: 1 }
    }

    /// Whether this is version 1 of the tree encoding, the encoding an
    /// object without the field holds.
    #[must_use]
    pub const fn is_default(&self) -> bool {
        matches!(self.shape, InstanceShape::Tree) && self.version == 1
    }
}

impl Default for PayloadEncoding {
    fn default() -> Self {
        Self::TREE
    }
}

impl std::fmt::Display for PayloadEncoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.shape.name(), self.version)
    }
}

/// An instance stored in the VCS: a sequence of trees, one table instance
/// or one graph instance, conforming to a schema.
///
/// [`crate::payload`] encodes and decodes the payload by its
/// [`encoding`](Self::encoding).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataSetObject {
    /// Which schema this data conforms to.
    pub schema_id: ObjectId,
    /// The encoded instance.
    pub data: Vec<u8>,
    /// Number of records: trees for a tree payload, rows for a table
    /// payload, nodes for a graph payload.
    pub record_count: u64,
    /// Caller key identifying the record(s) this set holds, so a
    /// committed data set can be mapped back to a downstream key (a
    /// source path, an AT-URI, or any caller-chosen identifier). `None`
    /// when no key was recorded. Carried forward across data migration.
    #[serde(default)]
    pub key: Option<String>,
    /// How [`data`](Self::data) is encoded. Last, and omitted for the
    /// default tree encoding: an object is a positional array, so only a
    /// trailing field can be omitted without shifting the others.
    #[serde(default, skip_serializing_if = "PayloadEncoding::is_default")]
    pub encoding: PayloadEncoding,
}

/// A complement from data migration, enabling backward migration.
///
/// [`crate::data_mig`] states the conventions its two identifiers follow.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComplementObject {
    /// The migration that produced this complement.
    pub migration_id: ObjectId,
    /// The data set this complement was computed from.
    pub data_id: ObjectId,
    /// The encoded complement.
    pub complement: Vec<u8>,
    /// How [`complement`](Self::complement) is encoded; last, and omitted
    /// for the default tree encoding, as for [`DataSetObject::encoding`].
    #[serde(default, skip_serializing_if = "PayloadEncoding::is_default")]
    pub encoding: PayloadEncoding,
}

impl ComplementObject {
    /// The stored migration that produced this complement, or `None` when
    /// it names none: a complement whose `migration_id` is its `data_id`, as
    /// repositories written by version 0.78 hold, is the complement of that
    /// data set under a migration that is not known.
    #[must_use]
    pub fn migration(&self) -> Option<ObjectId> {
        (self.migration_id != self.data_id).then_some(self.migration_id)
    }
}

/// A CST complement for format-preserving round-trips.
///
/// Stores the tree-sitter CST Schema (which includes all formatting
/// information as constraints) alongside a data set, enabling
/// `emit_from_schema` to reconstruct the original file formatting
/// after schema migration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CstComplementObject {
    /// The data set this CST complement was captured from.
    pub data_id: ObjectId,
    /// MessagePack-encoded `CstComplement` (from `panproto_io::cst_extract`).
    pub cst_complement: Vec<u8>,
}

/// An edit log: a sequence of edits (tree edits, table deltas or graph
/// deltas) applied to a data set.
///
/// Edit logs are content-addressed by hashing the sequence of edits.
/// Two edit logs with the same edits hash to the same object, enabling
/// deduplication.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditLogObject {
    /// The schema these edits apply to.
    pub schema_id: ObjectId,
    /// The data set these edits were applied to.
    pub data_id: ObjectId,
    /// The encoded edits.
    pub edits: Vec<u8>,
    /// Number of edits in the log.
    pub edit_count: u64,
    /// Object ID of the complement state after all edits.
    pub final_complement: ObjectId,
    /// How [`edits`](Self::edits) is encoded; last, and omitted for the
    /// default tree encoding, as for [`DataSetObject::encoding`].
    #[serde(default, skip_serializing_if = "PayloadEncoding::is_default")]
    pub encoding: PayloadEncoding,
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
            key: Some("at://did:plc:abc/app.bsky.feed.post/1".to_owned()),
            encoding: PayloadEncoding::TREE,
        };
        let bytes = rmp_serde::to_vec(&ds)?;
        let ds2: DataSetObject = rmp_serde::from_slice(&bytes)?;
        assert_eq!(ds.schema_id, ds2.schema_id);
        assert_eq!(ds.data, ds2.data);
        assert_eq!(ds.record_count, ds2.record_count);
        assert_eq!(ds.key, ds2.key);
        Ok(())
    }

    #[test]
    fn complement_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let comp = ComplementObject {
            migration_id: ObjectId::from_bytes([1; 32]),
            data_id: ObjectId::from_bytes([2; 32]),
            complement: vec![10, 20, 30],
            encoding: PayloadEncoding::TREE,
        };
        let bytes = rmp_serde::to_vec(&comp)?;
        let comp2: ComplementObject = rmp_serde::from_slice(&bytes)?;
        assert_eq!(comp.migration_id, comp2.migration_id);
        assert_eq!(comp.data_id, comp2.data_id);
        assert_eq!(comp.complement, comp2.complement);
        Ok(())
    }

    #[test]
    fn edit_log_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let el = EditLogObject {
            schema_id: ObjectId::from_bytes([1; 32]),
            data_id: ObjectId::from_bytes([2; 32]),
            edits: vec![42, 43, 44],
            edit_count: 3,
            final_complement: ObjectId::from_bytes([3; 32]),
            encoding: PayloadEncoding::TREE,
        };
        let bytes = rmp_serde::to_vec(&el)?;
        let el2: EditLogObject = rmp_serde::from_slice(&bytes)?;
        assert_eq!(el.schema_id, el2.schema_id);
        assert_eq!(el.data_id, el2.data_id);
        assert_eq!(el.edits, el2.edits);
        assert_eq!(el.edit_count, el2.edit_count);
        assert_eq!(el.final_complement, el2.final_complement);
        Ok(())
    }

    #[test]
    fn commit_with_edit_logs() -> Result<(), Box<dyn std::error::Error>> {
        let commit = CommitObject::builder(ObjectId::ZERO, "test", "test", "test")
            .timestamp(0)
            .edit_log_ids(vec![
                ObjectId::from_bytes([10; 32]),
                ObjectId::from_bytes([11; 32]),
            ])
            .build();
        let bytes = rmp_serde::to_vec(&commit)?;
        let commit2: CommitObject = rmp_serde::from_slice(&bytes)?;
        assert_eq!(commit.edit_log_ids, commit2.edit_log_ids);
        Ok(())
    }

    #[test]
    fn commit_with_theory_ids() -> Result<(), Box<dyn std::error::Error>> {
        let mut theories = BTreeMap::new();
        theories.insert("ThGraph".to_owned(), ObjectId::from_bytes([5; 32]));
        let commit = CommitObject::builder(ObjectId::ZERO, "test", "test", "test")
            .timestamp(0)
            .theory_ids(theories)
            .build();
        let bytes = rmp_serde::to_vec(&commit)?;
        let commit2: CommitObject = rmp_serde::from_slice(&bytes)?;
        assert_eq!(commit.theory_ids, commit2.theory_ids);
        assert_eq!(
            commit2.theory_ids.get("ThGraph"),
            Some(&ObjectId::from_bytes([5; 32]))
        );
        Ok(())
    }

    #[test]
    fn flat_schema_round_trip_through_serde() -> Result<(), Box<dyn std::error::Error>> {
        use panproto_gat::Name;
        use panproto_schema::{Schema, Vertex};
        use std::collections::HashMap;

        let mut vertices = HashMap::new();
        vertices.insert(
            Name::from("root"),
            Vertex {
                id: Name::from("root"),
                kind: Name::from("object"),
                nsid: None,
            },
        );
        let schema = Schema {
            protocol: "flat-proto".into(),
            vertices,
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
            ..Default::default()
        };
        let obj = Object::FlatSchema(Box::new(schema.clone()));
        let bytes = rmp_serde::to_vec(&obj)?;
        let obj2: Object = rmp_serde::from_slice(&bytes)?;
        match obj2 {
            Object::FlatSchema(s) => {
                assert_eq!(s.protocol, schema.protocol);
                assert_eq!(s.vertices.len(), 1);
            }
            other => panic!("expected FlatSchema, got {}", other.type_name()),
        }
        Ok(())
    }

    #[test]
    fn single_leaf_and_directory_hash_distinctly() -> Result<(), Box<dyn std::error::Error>> {
        use crate::hash::hash_schema_tree;

        let leaf_id = ObjectId::from_bytes([9; 32]);

        let single = SchemaTreeObject::SingleLeaf {
            file_schema_id: leaf_id,
        };
        // A Directory with a single `File` entry carrying the same
        // leaf id: prior to the typed variants, the two forms could
        // have collapsed into ambiguous shapes.
        let dir = SchemaTreeObject::Directory {
            entries: vec![("only".to_owned(), SchemaTreeEntry::File(leaf_id))],
        };

        let h_single = hash_schema_tree(&single)?;
        let h_dir = hash_schema_tree(&dir)?;
        assert_ne!(
            h_single, h_dir,
            "SingleLeaf and Directory must hash to distinct ObjectIds"
        );
        Ok(())
    }

    #[test]
    fn commit_backward_compat_no_theory_ids() -> Result<(), Box<dyn std::error::Error>> {
        // Simulate a commit serialized before theory_ids existed.
        let commit_old = CommitObject::builder(ObjectId::ZERO, "test", "test", "test")
            .timestamp(0)
            .build();
        let bytes = rmp_serde::to_vec(&commit_old)?;
        let commit2: CommitObject = rmp_serde::from_slice(&bytes)?;
        assert!(commit2.theory_ids.is_empty());
        Ok(())
    }
}
