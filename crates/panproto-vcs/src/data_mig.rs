//! Data migration engine for the VCS.
//!
//! An instance of any shape migrates forward along a stored migration
//! ([`Object::Migration`]) by the get of its shape's lens, and backward by
//! that lens's put against the complement the forward migration stored. The
//! lens is the stored migration compiled between its two schemas
//! ([`StoredMigration::lens`]): a sequence of trees migrates record by record
//! through the tree lens, a table through the table lens and a graph through
//! the graph lens.
//!
//! Two conventions bind the stored objects to each other:
//!
//! - A [`ComplementObject`] records, as its `migration_id`, the stored
//!   migration that produced it, and as its `data_id` the data set it was
//!   computed from. A complement whose two fields are equal, as repositories
//!   written by version 0.78 hold, reads as the complement of that data set
//!   under a migration that is not known ([`ComplementObject::migration`]).
//! - A migrated [`DataSetObject`] records, as its `schema_id`, the schema tree
//!   of its schema as [`store_schema_as_tree`](crate::tree::store_schema_as_tree)
//!   stores it. A `schema_id` may also name a flat schema
//!   ([`Object::FlatSchema`]), as data migrated by version 0.78 does:
//!   [`resolve_schema`] reads either, and [`schema_identity`] gives a schema
//!   tree and a flat schema holding the same schema the same identity, which
//!   is what staleness compares.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Name;
use panproto_inst::BoundaryProtocols;
use panproto_inst::identity::IdentityPolicy;
use panproto_lens::{Lens, LensError};
use panproto_mig::Migration;
use panproto_schema::{Protocol, Schema};

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{
    CommitObject, ComplementObject, CstComplementObject, DataSetObject, InstanceShape, Object,
};
use crate::payload::{ComplementPayload, Instance};
use crate::store::Store;

/// A data set that is stale relative to the schema of its commit.
#[derive(Debug, Clone)]
pub struct StaleData {
    /// The object ID of the stale data set.
    pub data_id: ObjectId,
    /// The schema ID the data set records.
    pub data_schema_id: ObjectId,
    /// The schema ID the commit records.
    pub head_schema_id: ObjectId,
}

/// A refusal of a forward or backward data migration, naming what to repair.
///
/// The checks run in a fixed order, so one input has one refusal: the
/// objects named exist and are of their kinds (the data set, the complement
/// when migrating backward, the migration, its two schemas, the data set's
/// schema); the complement names its migration; the data set conforms to the
/// schema the migration starts from (forward) or reaches (backward); the
/// migration compiles; the complement holds the data set's shape and, for a
/// sequence of trees, one complement per record; and last the lens accepts
/// each record in order.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MigrationConflict {
    /// No object is stored under `id`.
    #[error("no {expected} is stored under {id}")]
    Missing {
        /// The identifier.
        id: ObjectId,
        /// The kind of object required.
        expected: &'static str,
    },
    /// The object stored under `id` is of another kind.
    #[error("{id} names a {found}, not a {expected}")]
    WrongKind {
        /// The identifier.
        id: ObjectId,
        /// The kind of object required.
        expected: &'static str,
        /// The kind of object stored.
        found: &'static str,
    },
    /// The complement names no migration, so it cannot be put back; the data
    /// set it was computed from is migrated forward again to replace it.
    #[error("complement {complement_id} names no migration; migrate data set {data_id} again")]
    UnknownMigration {
        /// The complement.
        complement_id: ObjectId,
        /// The data set it was computed from.
        data_id: ObjectId,
    },
    /// The data set conforms to another schema than the one the migration
    /// starts from (forward) or reaches (backward): it is stale, and is first
    /// migrated along the migration recorded from its own schema. Both
    /// schemas are given by their identities ([`schema_identity`]).
    #[error("data set {data_id} conforms to schema {schema}, not to {expected}")]
    SchemaMismatch {
        /// The data set.
        data_id: ObjectId,
        /// The identity of its schema.
        schema: ObjectId,
        /// The identity of the schema the migration requires.
        expected: ObjectId,
    },
    /// The migration does not compile between its two schemas.
    #[error("migration {migration_id} does not compile: {detail}")]
    MigrationDoesNotCompile {
        /// The migration.
        migration_id: ObjectId,
        /// The compiler's refusal.
        detail: String,
    },
    /// The complement holds another shape of complement than the data set
    /// holds.
    #[error("complement {complement_id} is a {complement:?} complement of a {data:?} data set")]
    ShapeMismatch {
        /// The complement.
        complement_id: ObjectId,
        /// The data set's shape.
        data: InstanceShape,
        /// The complement's shape.
        complement: InstanceShape,
    },
    /// A tree complement holds another number of complements than the data
    /// set holds records.
    #[error("complement {complement_id} holds {complements} complements for {records} records")]
    RecordCountMismatch {
        /// The complement.
        complement_id: ObjectId,
        /// The records of the data set.
        records: usize,
        /// The complements the complement object holds.
        complements: usize,
    },
    /// The lens's get refuses the instance, or the record of a sequence of
    /// trees.
    #[error("data set {data_id} is outside the domain of get: {detail}")]
    OutsideGetDomain {
        /// The data set.
        data_id: ObjectId,
        /// The refused record, for a sequence of trees.
        record: Option<usize>,
        /// The lens's refusal.
        detail: String,
    },
    /// The complement is not bound to the lens of its migration.
    #[error("complement {complement_id} is not bound to its migration's lens: {detail}")]
    ComplementIdentity {
        /// The complement.
        complement_id: ObjectId,
        /// The refused record, for a sequence of trees.
        record: Option<usize>,
        /// The identity check's refusal.
        detail: String,
    },
    /// The lens's put refuses the migrated instance against its complement.
    #[error("data set {data_id} is outside the domain of put against {complement_id}: {detail}")]
    OutsidePutDomain {
        /// The migrated data set.
        data_id: ObjectId,
        /// The complement.
        complement_id: ObjectId,
        /// The refused record, for a sequence of trees.
        record: Option<usize>,
        /// The lens's refusal.
        detail: String,
    },
    /// No commit of the first-parent chain of `commit` (itself included)
    /// has a schema with the identity `schema`, so no recorded migration
    /// starts where the data set does.
    #[error("no commit on the first-parent chain of {commit} has schema {schema}")]
    NoMigrationPath {
        /// The commit whose chain was walked.
        commit: ObjectId,
        /// The identity of the data set's schema.
        schema: ObjectId,
    },
    /// The commit changes its first parent's schema and records no
    /// migration, so the path through it has no link.
    #[error("commit {commit} changes its parent's schema and records no migration")]
    MissingLink {
        /// The commit.
        commit: ObjectId,
    },
    /// The migration the commit records does not run from its first
    /// parent's schema to its own.
    #[error(
        "commit {commit} records migration {migration}, which does not run from its parent's schema to its own"
    )]
    LinkMismatch {
        /// The commit.
        commit: ObjectId,
        /// The migration it records.
        migration: ObjectId,
    },
}

fn conflict(conflict: MigrationConflict) -> VcsError {
    VcsError::Migration(Box::new(conflict))
}

/// The object stored under `id`, or [`MigrationConflict::Missing`].
fn fetch(store: &dyn Store, id: ObjectId, expected: &'static str) -> Result<Object, VcsError> {
    match store.get(&id) {
        Err(VcsError::ObjectNotFound { .. }) => {
            Err(conflict(MigrationConflict::Missing { id, expected }))
        }
        found => found,
    }
}

fn wrong_kind(id: ObjectId, expected: &'static str, found: &Object) -> VcsError {
    conflict(MigrationConflict::WrongKind {
        id,
        expected,
        found: found.type_name(),
    })
}

/// The schema `id` names: the assembled schema of a schema tree, or a flat
/// schema.
///
/// # Errors
///
/// [`VcsError::WrongObjectType`] when `id` names another kind of object, and
/// the store's errors.
pub fn resolve_schema(store: &dyn Store, id: &ObjectId) -> Result<Schema, VcsError> {
    match store.get(id)? {
        Object::SchemaTree(_) => {
            crate::tree::assemble_schema_dyn(store, id, &crate::tree::project_coproduct_protocol())
        }
        Object::FlatSchema(schema) => Ok(*schema),
        other => Err(VcsError::WrongObjectType {
            expected: "schema tree or flat schema",
            found: other.type_name(),
        }),
    }
}

/// The identity of the schema `id` names: the identifier that schema has as
/// a flat schema ([`hash_schema`](crate::hash::hash_schema)). A schema tree
/// and a flat schema holding the same schema have the same identity.
///
/// # Errors
///
/// As [`resolve_schema`].
pub fn schema_identity(store: &dyn Store, id: &ObjectId) -> Result<ObjectId, VcsError> {
    crate::hash::hash_schema(&resolve_schema(store, id)?)
}

/// The schema `id` names, refusing a missing object or another kind of
/// object as a [`MigrationConflict`].
fn schema_of(store: &dyn Store, id: ObjectId) -> Result<Schema, VcsError> {
    match fetch(store, id, "schema")? {
        Object::SchemaTree(_) => resolve_schema(store, &id),
        Object::FlatSchema(schema) => Ok(*schema),
        other => Err(wrong_kind(id, "schema", &other)),
    }
}

/// Store `mapping` as the migration from `src` to `tgt`.
///
/// Both schemas are stored as flat schemas, and the migration, its endpoints
/// named by their identifiers, as an [`Object::Migration`] between them.
/// Returns the migration's identifier, which is a function of the two
/// schemas and the mapping.
///
/// # Errors
///
/// The store's errors.
pub fn store_migration(
    store: &mut dyn Store,
    src: &Schema,
    tgt: &Schema,
    mapping: Migration,
) -> Result<ObjectId, VcsError> {
    let src_id = store.put(&Object::FlatSchema(Box::new(src.clone())))?;
    let tgt_id = store.put(&Object::FlatSchema(Box::new(tgt.clone())))?;
    let mapping = mapping.with_endpoints(
        Some(Name::from(src_id.to_string())),
        Some(Name::from(tgt_id.to_string())),
    );
    store.put(&Object::Migration {
        src: src_id,
        tgt: tgt_id,
        mapping,
    })
}

/// A stored migration with its two schemas.
#[derive(Clone, Debug)]
pub struct StoredMigration {
    /// The migration's identifier.
    pub id: ObjectId,
    /// The schema it starts from.
    pub src: Schema,
    /// The schema it reaches.
    pub tgt: Schema,
    /// The identity of `src` ([`schema_identity`]).
    pub src_identity: ObjectId,
    /// The identity of `tgt`.
    pub tgt_identity: ObjectId,
    /// The mapping.
    pub mapping: Migration,
}

impl StoredMigration {
    /// The lens the migration defines: the mapping compiled between the two
    /// schemas.
    ///
    /// # Errors
    ///
    /// [`MigrationConflict::MigrationDoesNotCompile`] when the mapping does
    /// not compile.
    pub fn lens(&self) -> Result<Lens, VcsError> {
        let compiled = panproto_mig::compile(&self.src, &self.tgt, &self.mapping).map_err(|e| {
            conflict(MigrationConflict::MigrationDoesNotCompile {
                migration_id: self.id,
                detail: e.to_string(),
            })
        })?;
        Ok(Lens {
            compiled,
            src_schema: self.src.clone(),
            tgt_schema: self.tgt.clone(),
        })
    }
}

/// The migration stored under `id`, with its two schemas.
///
/// # Errors
///
/// [`MigrationConflict::Missing`] or [`MigrationConflict::WrongKind`] when
/// `id` or one of the migration's schemas names no object or an object of
/// another kind, and the store's errors.
pub fn load_migration(store: &dyn Store, id: ObjectId) -> Result<StoredMigration, VcsError> {
    let (src_id, tgt_id, mapping) = match fetch(store, id, "migration")? {
        Object::Migration { src, tgt, mapping } => (src, tgt, mapping),
        other => return Err(wrong_kind(id, "migration", &other)),
    };
    let src = schema_of(store, src_id)?;
    let tgt = schema_of(store, tgt_id)?;
    Ok(StoredMigration {
        id,
        src_identity: crate::hash::hash_schema(&src)?,
        tgt_identity: crate::hash::hash_schema(&tgt)?,
        src,
        tgt,
        mapping,
    })
}

/// The data set stored under `id`.
fn data_set(store: &dyn Store, id: ObjectId) -> Result<DataSetObject, VcsError> {
    match fetch(store, id, "data set")? {
        Object::DataSet(dataset) => Ok(dataset),
        other => Err(wrong_kind(id, "data set", &other)),
    }
}

/// Refuse `dataset` unless its schema has the identity `expected`.
fn conforms(
    store: &dyn Store,
    data_id: ObjectId,
    dataset: &DataSetObject,
    expected: ObjectId,
) -> Result<(), VcsError> {
    let schema = crate::hash::hash_schema(&schema_of(store, dataset.schema_id)?)?;
    if schema == expected {
        Ok(())
    } else {
        Err(conflict(MigrationConflict::SchemaMismatch {
            data_id,
            schema,
            expected,
        }))
    }
}

/// The get of `instance` along `lens`, with the complements get produced.
///
/// Each tree of a sequence of trees goes through the tree lens, a table
/// through the table lens under `protocol` at both boundaries, and a graph
/// through the graph lens.
///
/// # Errors
///
/// [`MigrationConflict::OutsideGetDomain`] naming `data_id` and, for a
/// sequence of trees, the first refused record.
pub fn get_instance(
    lens: &Lens,
    instance: &Instance,
    protocol: &Protocol,
    data_id: ObjectId,
) -> Result<(Instance, ComplementPayload), VcsError> {
    let refused = |record: Option<usize>, error: LensError| {
        conflict(MigrationConflict::OutsideGetDomain {
            data_id,
            record,
            detail: error.to_string(),
        })
    };
    Ok(match instance {
        Instance::Tree(trees) => {
            let mut views = Vec::with_capacity(trees.len());
            let mut complements = Vec::with_capacity(trees.len());
            for (record, tree) in trees.iter().enumerate() {
                let (view, complement) =
                    panproto_lens::get(lens, tree).map_err(|e| refused(Some(record), e))?;
                views.push(view);
                complements.push(complement);
            }
            (Instance::Tree(views), ComplementPayload::Tree(complements))
        }
        Instance::Table(table) => {
            let protocols = BoundaryProtocols {
                src: protocol,
                tgt: protocol,
            };
            let (view, complement) = panproto_lens::table_lens::get(lens, table, protocols)
                .map_err(|e| refused(None, e))?;
            (
                Instance::Table(view),
                ComplementPayload::Table(Box::new(complement)),
            )
        }
        Instance::Graph(graph) => {
            let (view, complement) =
                panproto_lens::graph_lens::get(lens, graph).map_err(|e| refused(None, e))?;
            (
                Instance::Graph(view),
                ComplementPayload::Graph(Box::new(complement)),
            )
        }
    })
}

/// Migrate the data set `data_id` forward along the stored migration
/// `migration_id`.
///
/// The instance is put through the get of its shape's lens
/// ([`StoredMigration::lens`]); `protocol` describes both boundaries of a
/// table lens and is not read for the other shapes. The migrated data set
/// records the schema tree of the migration's target schema and the data
/// set's key; the complement object records the complement under its shape's
/// encoding, bound by the lens to the two schemas and the compiled migration,
/// with `migration_id` and `data_id`. Both are stored.
///
/// Returns `(migrated_data_id, complement_id)`.
///
/// # Errors
///
/// A [`MigrationConflict`] in the order that type states, the errors of
/// decoding the payload, and the store's errors.
pub fn migrate_forward(
    store: &mut dyn Store,
    data_id: ObjectId,
    migration_id: ObjectId,
    protocol: &Protocol,
) -> Result<(ObjectId, ObjectId), VcsError> {
    let dataset = data_set(store, data_id)?;
    let migration = load_migration(store, migration_id)?;
    conforms(store, data_id, &dataset, migration.src_identity)?;
    let lens = migration.lens()?;
    let (view, complement) = get_instance(&lens, &dataset.decode()?, protocol, data_id)?;
    let schema_id = crate::tree::store_schema_as_tree(store, migration.tgt)?;
    let migrated = DataSetObject::from_instance(schema_id, &view, dataset.key)?;
    let migrated_id = store.put(&Object::DataSet(migrated))?;
    let complement = ComplementObject::from_payload(migration_id, data_id, &complement)?;
    let complement_id = store.put(&Object::Complement(complement))?;
    Ok((migrated_id, complement_id))
}

/// Migrate the data set `data_id` backward against the stored complement
/// `complement_id`.
///
/// The migration is the one the complement names. The instance and the
/// complement are put through the put of their shape's lens under
/// [`IdentityPolicy::Strict`], so a complement must be bound to that lens;
/// `protocol` describes both boundaries of a table lens. The restored data
/// set records the schema tree of the migration's source schema and the
/// data set's key, and is stored; its identifier is returned.
///
/// # Errors
///
/// A [`MigrationConflict`] in the order that type states, the errors of
/// decoding the payloads, and the store's errors.
pub fn migrate_backward(
    store: &mut dyn Store,
    data_id: ObjectId,
    complement_id: ObjectId,
    protocol: &Protocol,
) -> Result<ObjectId, VcsError> {
    let dataset = data_set(store, data_id)?;
    let complement = match fetch(store, complement_id, "complement")? {
        Object::Complement(complement) => complement,
        other => return Err(wrong_kind(complement_id, "complement", &other)),
    };
    let migration_id = complement.migration().ok_or_else(|| {
        conflict(MigrationConflict::UnknownMigration {
            complement_id,
            data_id: complement.data_id,
        })
    })?;
    let migration = load_migration(store, migration_id)?;
    conforms(store, data_id, &dataset, migration.tgt_identity)?;
    let lens = migration.lens()?;
    let refused = |record: Option<usize>, error: LensError| {
        conflict(match error {
            LensError::Identity(error) => MigrationConflict::ComplementIdentity {
                complement_id,
                record,
                detail: error.to_string(),
            },
            other => MigrationConflict::OutsidePutDomain {
                data_id,
                complement_id,
                record,
                detail: other.to_string(),
            },
        })
    };
    let restored = match (dataset.decode()?, complement.decode()?) {
        (Instance::Tree(views), ComplementPayload::Tree(complements)) => {
            if views.len() != complements.len() {
                return Err(conflict(MigrationConflict::RecordCountMismatch {
                    complement_id,
                    records: views.len(),
                    complements: complements.len(),
                }));
            }
            let mut trees = Vec::with_capacity(views.len());
            for (record, (view, complement)) in views.iter().zip(&complements).enumerate() {
                let (tree, _) = panproto_lens::asymmetric::put_checked(
                    &lens,
                    view,
                    complement,
                    IdentityPolicy::Strict,
                )
                .map_err(|e| refused(Some(record), e))?;
                trees.push(tree);
            }
            Instance::Tree(trees)
        }
        (Instance::Table(view), ComplementPayload::Table(complement)) => {
            let protocols = BoundaryProtocols {
                src: protocol,
                tgt: protocol,
            };
            let (table, _) = panproto_lens::table_lens::put_checked(
                &lens,
                &view,
                &complement,
                protocols,
                IdentityPolicy::Strict,
            )
            .map_err(|e| refused(None, e))?;
            Instance::Table(table)
        }
        (Instance::Graph(view), ComplementPayload::Graph(complement)) => {
            let (graph, _) = panproto_lens::graph_lens::put_checked(
                &lens,
                &view,
                &complement,
                IdentityPolicy::Strict,
            )
            .map_err(|e| refused(None, e))?;
            Instance::Graph(graph)
        }
        (view, complement) => {
            return Err(conflict(MigrationConflict::ShapeMismatch {
                complement_id,
                data: view.shape(),
                complement: complement.shape(),
            }));
        }
    };
    let schema_id = crate::tree::store_schema_as_tree(store, migration.src)?;
    let restored = DataSetObject::from_instance(schema_id, &restored, dataset.key)?;
    store.put(&Object::DataSet(restored))
}

/// Check which data sets in a commit are stale relative to its schema.
///
/// A data set is stale when the identity of its schema
/// ([`schema_identity`]) is not that of the commit's schema: the data was
/// written against another schema and is migrated before it is read as the
/// commit's.
///
/// # Errors
///
/// Returns `VcsError` if a data set or schema cannot be loaded from the
/// store.
pub fn detect_staleness(
    store: &dyn Store,
    commit: &crate::object::CommitObject,
) -> Result<Vec<StaleData>, VcsError> {
    let head = schema_identity(store, &commit.schema_id)?;
    let mut stale = Vec::new();
    for data_id in &commit.data_ids {
        let Object::DataSet(data_obj) = store.get(data_id)? else {
            continue;
        };
        if schema_identity(store, &data_obj.schema_id)? != head {
            stale.push(StaleData {
                data_id: *data_id,
                data_schema_id: data_obj.schema_id,
                head_schema_id: commit.schema_id,
            });
        }
    }
    Ok(stale)
}

/// The commit stored under `id`, refusing a missing object or another kind
/// of object as a [`MigrationConflict`].
fn commit_at(store: &dyn Store, id: ObjectId) -> Result<CommitObject, VcsError> {
    match fetch(store, id, "commit")? {
        Object::Commit(commit) => Ok(commit),
        other => Err(wrong_kind(id, "commit", &other)),
    }
}

/// The migrations first-parent history records from the schema with the
/// identity `schema` to the schema of the commit `commit_id`, oldest first.
///
/// The walk follows first parents from `commit_id` (itself included) to the
/// nearest commit whose schema has the identity `schema`, the path's start.
/// Each later commit of the walk, from the oldest, is a link: a commit whose
/// schema has its first parent's schema's identity changes nothing and adds
/// no migration; any other commit adds the migration it records, which must
/// run from its first parent's schema to its own (compared by
/// [`schema_identity`]). A data set of the start's schema migrated along the
/// path by [`migrate_along`] conforms to `commit_id`'s schema.
///
/// # Errors
///
/// [`MigrationConflict::NoMigrationPath`] when no commit of the chain has a
/// schema of identity `schema`; for the oldest link that fails,
/// [`MigrationConflict::MissingLink`] when it records no migration and
/// [`MigrationConflict::LinkMismatch`] when its migration runs between other
/// schemas; [`MigrationConflict::Missing`] or [`MigrationConflict::WrongKind`]
/// when a commit or a recorded migration or its schema is absent or of
/// another kind; and the errors of reading a schema.
pub fn migration_path(
    store: &dyn Store,
    commit_id: ObjectId,
    schema: ObjectId,
) -> Result<Vec<ObjectId>, VcsError> {
    let mut chain: Vec<(ObjectId, Option<ObjectId>, ObjectId)> = Vec::new();
    let mut at = Some(commit_id);
    let mut found = false;
    while let Some(id) = at {
        let commit = commit_at(store, id)?;
        let identity = schema_identity(store, &commit.schema_id)?;
        chain.push((id, commit.migration_id, identity));
        if identity == schema {
            found = true;
            break;
        }
        at = commit.parents.first().copied();
    }
    if !found {
        return Err(conflict(MigrationConflict::NoMigrationPath {
            commit: commit_id,
            schema,
        }));
    }
    let mut path = Vec::new();
    for link in chain.windows(2).rev() {
        let [(commit, recorded, identity), (_, _, parent_identity)] = link else {
            continue;
        };
        if identity == parent_identity {
            continue;
        }
        let migration_id =
            recorded.ok_or_else(|| conflict(MigrationConflict::MissingLink { commit: *commit }))?;
        let migration = load_migration(store, migration_id)?;
        if migration.src_identity != *parent_identity || migration.tgt_identity != *identity {
            return Err(conflict(MigrationConflict::LinkMismatch {
                commit: *commit,
                migration: migration_id,
            }));
        }
        path.push(migration_id);
    }
    Ok(path)
}

/// The result of migrating a data set along a path of stored migrations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigratedAlong {
    /// The data set at the path's end: the one migrated, when the path is
    /// empty.
    pub data_id: ObjectId,
    /// The complement each step stored, in path order.
    pub complement_ids: Vec<ObjectId>,
}

/// Migrate the data set `data_id` forward along `path`, step by step.
///
/// Each step is [`migrate_forward`] along the next migration, applied to the
/// data set the previous step stored; a table's step runs under the protocol
/// `protocol_for` gives the step's source schema, which no other shape
/// reads. The data set at the end is the get of
/// the composite of the steps' lenses, and each step's complement is kept,
/// so the data set migrates back by [`migrate_backward`] against them in
/// reverse order.
///
/// # Errors
///
/// The [`MigrationConflict`] of the first step that refuses, in the order
/// [`migrate_forward`] checks, and the errors of `protocol_for`, of
/// decoding and of the store.
pub fn migrate_along(
    store: &mut dyn Store,
    data_id: ObjectId,
    path: &[ObjectId],
    protocol_for: &mut dyn FnMut(&Schema) -> Result<Protocol, VcsError>,
) -> Result<MigratedAlong, VcsError> {
    let mut out = MigratedAlong {
        data_id,
        complement_ids: Vec::with_capacity(path.len()),
    };
    for migration_id in path {
        let protocol = if data_set(store, out.data_id)?.encoding.shape == InstanceShape::Table {
            protocol_for(&load_migration(store, *migration_id)?.src)?
        } else {
            Protocol::default()
        };
        let (next, complement) = migrate_forward(store, out.data_id, *migration_id, &protocol)?;
        out.data_id = next;
        out.complement_ids.push(complement);
    }
    Ok(out)
}

/// One side of a merge, as a merge migration reads it.
#[derive(Clone, Debug)]
pub struct MergeSide {
    /// The side's commit: ours, or theirs (the tip and the replayed commit
    /// of a replay).
    pub commit: ObjectId,
    /// The stored migration from the side's schema to the merged schema, or
    /// `None` when the side's schema has the merged schema's identity.
    pub migration: Option<ObjectId>,
    /// The protocol object the side's commit records, which resolves a
    /// table lens's protocol before `registered`.
    pub protocol: Option<Protocol>,
}

/// One side of a merge as its merge migration reads it.
///
/// It is the side's commit `commit_id`, the stored migration `mapping` from
/// the side's `schema` to `merged` (stored here, and none when the two
/// schemas have one identity), and the protocol object `commit` records.
///
/// # Errors
///
/// The store's errors, and [`VcsError::WrongObjectType`] when the
/// commit's protocol object is another kind of object.
pub fn merge_side(
    store: &mut dyn Store,
    (commit_id, commit): (ObjectId, &CommitObject),
    (schema, mapping): (&Schema, &Migration),
    merged: &Schema,
) -> Result<MergeSide, VcsError> {
    let migration = if crate::hash::hash_schema(schema)? == crate::hash::hash_schema(merged)? {
        None
    } else {
        Some(store_migration(store, schema, merged, mapping.clone())?)
    };
    Ok(MergeSide {
        commit: commit_id,
        migration,
        protocol: commit_protocol(store, commit)?,
    })
}

/// The migration of the instances a merge (or replay) combines to its
/// merged schema, along first-parent history and the merge's migrations.
#[derive(Debug)]
pub struct MergeMigration<'a, S> {
    /// Our side, then theirs.
    pub sides: [MergeSide; 2],
    /// The identity of the merged schema.
    pub merged: ObjectId,
    /// The protocols a caller registered, by name.
    pub registered: &'a HashMap<String, Protocol, S>,
}

impl<S: BuildHasher> MergeMigration<'_, S> {
    /// The stored migrations from the schema with identity `schema`, the
    /// schema of a data set the merge's `version` tracks, to the merged
    /// schema, and the side whose chain they follow.
    ///
    /// A schema with the merged schema's identity has the empty path.
    /// Otherwise the path is the [`migration_path`] from `schema` to a
    /// side's commit followed by that side's migration to the merged
    /// schema: our side's for our version, their side's for theirs, and for
    /// the base's version our side's when our commit's first-parent chain
    /// holds a commit of that schema, else their side's.
    ///
    /// # Errors
    ///
    /// As [`migration_path`]; for the base's version, the
    /// [`MigrationConflict::NoMigrationPath`] of our side when neither chain
    /// holds a commit of that schema.
    pub fn path(
        &self,
        store: &dyn Store,
        version: crate::instance_merge::Version,
        schema: ObjectId,
    ) -> Result<(Vec<ObjectId>, usize), VcsError> {
        use crate::instance_merge::Version;
        if schema == self.merged {
            return Ok((Vec::new(), usize::from(version == Version::Theirs)));
        }
        let sides: &[usize] = match version {
            Version::Base => &[0, 1],
            Version::Ours => &[0],
            Version::Theirs => &[1],
        };
        let mut first_refusal = None;
        for side in sides {
            match migration_path(store, self.sides[*side].commit, schema) {
                Ok(mut path) => {
                    path.extend(self.sides[*side].migration);
                    return Ok((path, *side));
                }
                Err(VcsError::Migration(refusal))
                    if matches!(*refusal, MigrationConflict::NoMigrationPath { .. }) =>
                {
                    first_refusal.get_or_insert(VcsError::Migration(refusal));
                }
                Err(other) => return Err(other),
            }
        }
        Err(first_refusal.unwrap_or(VcsError::NoPath))
    }

    /// The instance of the data set `data_id`, tracked by the merge's
    /// `version`, migrated to the merged schema: the get of each migration
    /// of [`path`](Self::path) in turn, or the refusal of the first that
    /// refuses. A table's get runs under the protocol its source schema
    /// names (resolved through the side's protocol object, then
    /// `registered`, then the registry), which no other shape reads.
    /// Nothing is stored.
    ///
    /// # Errors
    ///
    /// The store's errors, those of decoding the payload, and
    /// [`VcsError::UnknownProtocol`]; a [`MigrationConflict`] is returned as
    /// the inner error.
    pub fn migrate(
        &self,
        store: &dyn Store,
        version: crate::instance_merge::Version,
        data_id: ObjectId,
        dataset: &DataSetObject,
    ) -> Result<Result<Instance, MigrationConflict>, VcsError> {
        match self.migrated(store, version, data_id, dataset) {
            Ok(instance) => Ok(Ok(instance)),
            Err(VcsError::Migration(refusal)) => Ok(Err(*refusal)),
            Err(other) => Err(other),
        }
    }

    fn migrated(
        &self,
        store: &dyn Store,
        version: crate::instance_merge::Version,
        data_id: ObjectId,
        dataset: &DataSetObject,
    ) -> Result<Instance, VcsError> {
        let identity = schema_identity(store, &dataset.schema_id)?;
        let (path, side) = self.path(store, version, identity)?;
        let mut instance = dataset.decode()?;
        for migration_id in path {
            let migration = load_migration(store, migration_id)?;
            let protocol = if matches!(instance, Instance::Table(_)) {
                protocol_for_schema(
                    &migration.src,
                    ProtocolSources {
                        stored: self.sides[side].protocol.as_ref(),
                        registered: self.registered,
                    },
                )?
            } else {
                Protocol::default()
            };
            let lens = migration.lens()?;
            instance = get_instance(&lens, &instance, &protocol, data_id)?.0;
        }
        Ok(instance)
    }
}

/// Migrate all JSON files in a directory from one schema to another.
///
/// Each `.json` file is parsed as an instance of `src_schema`, migrated
/// forward through a lens to `tgt_schema`, and written back in place.
///
/// # Errors
///
/// Returns `VcsError::DataMigrationFailed` if lens generation, parsing,
/// or migration fails. Returns `VcsError::IoError` on filesystem errors.
pub fn migrate_data_directory(
    store: &mut dyn Store,
    data_dir: &std::path::Path,
    src_schema: &Schema,
    tgt_schema: &Schema,
    protocol: &Protocol,
) -> Result<(), VcsError> {
    let config = panproto_lens::AutoLensConfig::default();
    let result =
        panproto_lens::auto_generate(src_schema, tgt_schema, protocol, &config).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("lens: {e}"),
            }
        })?;

    for entry in std::fs::read_dir(data_dir).map_err(|e| VcsError::IoError(e.to_string()))? {
        let entry = entry.map_err(|e| VcsError::IoError(e.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let data: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&path).map_err(|e| VcsError::IoError(e.to_string()))?,
        )
        .map_err(|e| VcsError::DataMigrationFailed {
            reason: format!("parse {}: {e}", path.display()),
        })?;

        // Parse, migrate, write back
        let root = infer_root(src_schema);
        let instance =
            panproto_inst::parse_json(src_schema, protocol, &root, &data).map_err(|e| {
                VcsError::DataMigrationFailed {
                    reason: format!("parse instance: {e}"),
                }
            })?;

        let (view, _complement) = panproto_lens::get(&result.lens, &instance).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("lens get: {e}"),
            }
        })?;

        let output = panproto_inst::to_json(tgt_schema, &view).map_err(|e| {
            VcsError::DataMigrationFailed {
                reason: format!("serialize view: {e}"),
            }
        })?;

        // Store the migrated data, under the schema tree of its schema.
        let tgt_schema_id = crate::tree::store_schema_as_tree(store, tgt_schema.clone())?;
        let migrated_data = DataSetObject {
            schema_id: tgt_schema_id,
            data: rmp_serde::to_vec(&vec![view.clone()]).map_err(|e| {
                VcsError::DataMigrationFailed {
                    reason: format!("serialize: {e}"),
                }
            })?,
            record_count: 1,
            // The source file path is this set's natural key, matching
            // how `Repository::add_data` keys a staged file by default.
            key: Some(path.to_string_lossy().into_owned()),
            encoding: crate::object::PayloadEncoding::TREE,
        };
        store.put(&Object::DataSet(migrated_data))?;

        let pretty =
            serde_json::to_string_pretty(&output).map_err(|e| VcsError::DataMigrationFailed {
                reason: format!("serialize: {e}"),
            })?;

        std::fs::write(&path, pretty).map_err(|e| VcsError::IoError(e.to_string()))?;
    }

    Ok(())
}

/// Infer the root vertex of a schema.
///
/// Finds a vertex with no incoming edges; falls back to the first vertex
/// in iteration order if all vertices have incoming edges.
fn infer_root(schema: &Schema) -> String {
    for id in schema.vertices.keys() {
        let has_incoming = schema
            .incoming
            .get(id)
            .is_some_and(|edges| !edges.is_empty());
        if !has_incoming {
            return id.to_string();
        }
    }
    schema
        .vertices
        .keys()
        .next()
        .map_or_else(|| "root".to_owned(), std::string::ToString::to_string)
}

/// The places a protocol name is resolved in, read in this order by
/// [`resolve_protocol`].
#[derive(Debug)]
pub struct ProtocolSources<'a, S> {
    /// The protocol object of the repository state at hand: the one staged
    /// for the next commit, else the one the commit records
    /// ([`CommitObject::protocol_id`]).
    pub stored: Option<&'a Protocol>,
    /// The protocols a caller registered, by name
    /// ([`Repository::set_protocol`](crate::Repository::set_protocol)).
    pub registered: &'a HashMap<String, Protocol, S>,
}

impl<S> Clone for ProtocolSources<'_, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for ProtocolSources<'_, S> {}

/// Resolve the protocol named by `name`.
///
/// Resolution first checks the stored protocol object, then the protocol
/// registered under `name`, then the `panproto-protocols` registry by
/// canonical name or alias.
///
/// # Errors
///
/// [`VcsError::UnknownProtocol`] when none of the three resolves `name`.
pub fn resolve_protocol<S: BuildHasher>(
    name: &str,
    sources: ProtocolSources<'_, S>,
) -> Result<Protocol, VcsError> {
    if let Some(stored) = sources.stored.filter(|p| p.name == name) {
        return Ok(stored.clone());
    }
    if let Some(registered) = sources.registered.get(name) {
        return Ok(registered.clone());
    }
    if let Some(descriptor) = panproto_protocols::registry::descriptor(name) {
        return Ok((descriptor.protocol)());
    }
    Err(VcsError::UnknownProtocol {
        name: name.to_owned(),
    })
}

/// The protocol of `schema`: its protocol name resolved by
/// [`resolve_protocol`], so that data validation and data migration read
/// one protocol.
///
/// # Errors
///
/// [`VcsError::UnknownProtocol`] when the name does not resolve.
pub fn protocol_for_schema<S: BuildHasher>(
    schema: &Schema,
    sources: ProtocolSources<'_, S>,
) -> Result<Protocol, VcsError> {
    resolve_protocol(&schema.protocol, sources)
}

/// The protocol object `commit` records, if it records one.
///
/// # Errors
///
/// A store error, or [`VcsError::WrongObjectType`] when the recorded
/// object is not a protocol.
pub fn commit_protocol(
    store: &dyn Store,
    commit: &CommitObject,
) -> Result<Option<Protocol>, VcsError> {
    let Some(id) = commit.protocol_id else {
        return Ok(None);
    };
    match store.get(&id)? {
        Object::Protocol(protocol) => Ok(Some(*protocol)),
        other => Err(VcsError::WrongObjectType {
            expected: "protocol",
            found: other.type_name(),
        }),
    }
}

/// Lift a commit's versioned data sets through a schema change.
///
/// Returns the data-set and complement object ids the replayed, merged,
/// or cherry-picked commit should carry. When `src_schema` and
/// `tgt_schema` are the same schema, `commit.data_ids` and
/// `commit.complement_ids` are returned verbatim, so a replay or merge
/// that does not change the schema preserves the exact object
/// references. Otherwise `mapping` is stored as the migration from
/// `src_schema` to `tgt_schema` ([`store_migration`]), each
/// [`DataSetObject`] in `commit.data_ids` is migrated forward along it
/// ([`migrate_forward`], with [`protocol_for_schema`] over the protocol
/// `commit` records and `registered` for a table lens's protocol), and the
/// returned complement ids are the commit's original complements extended
/// with the complement stored for each lifted data set.
///
/// `cst_complement_ids` are not returned here: CST complements capture
/// formatting keyed by content, not schema, so callers propagate them
/// unchanged.
///
/// # Errors
///
/// The [`MigrationConflict`] of the first data set that does not migrate,
/// so the caller aborts rather than dropping data, and
/// [`VcsError::UnknownProtocol`] when the source schema's protocol name
/// does not resolve.
pub fn lift_commit_data<S: BuildHasher>(
    store: &mut dyn Store,
    commit: &CommitObject,
    (src_schema, tgt_schema): (&Schema, &Schema),
    mapping: &Migration,
    registered: &HashMap<String, Protocol, S>,
) -> Result<(Vec<ObjectId>, Vec<ObjectId>), VcsError> {
    // An unchanged schema lifts data to itself, so keep the exact object
    // references. A commit with no data needs no protocol.
    if commit.data_ids.is_empty()
        || crate::hash::hash_schema(src_schema)? == crate::hash::hash_schema(tgt_schema)?
    {
        return Ok((commit.data_ids.clone(), commit.complement_ids.clone()));
    }

    let stored = commit_protocol(store, commit)?;
    let protocol = protocol_for_schema(
        src_schema,
        ProtocolSources {
            stored: stored.as_ref(),
            registered,
        },
    )?;
    let migration_id = store_migration(store, src_schema, tgt_schema, mapping.clone())?;
    let mut new_data_ids = Vec::with_capacity(commit.data_ids.len());
    let mut new_complement_ids = commit.complement_ids.clone();
    for data_id in &commit.data_ids {
        let (lifted_data_id, complement_id) =
            migrate_forward(store, *data_id, migration_id, &protocol)?;
        new_data_ids.push(lifted_data_id);
        new_complement_ids.push(complement_id);
    }
    Ok((new_data_ids, new_complement_ids))
}

// ── CST complement pass-through ───────────────────────────────────────

/// Pass a CST complement through a schema migration.
///
/// CST complements are orthogonal to schema migrations: they capture
/// formatting information that is independent of schema structure.
/// During forward migration, the complement is re-stored with its
/// `data_id` updated to point to the new (migrated) data set.
///
/// # Errors
///
/// Returns `VcsError` if loading or storing fails.
pub fn pass_through_cst_complement(
    store: &mut dyn Store,
    old_cst_complement_id: ObjectId,
    new_data_id: ObjectId,
) -> Result<ObjectId, VcsError> {
    let old_comp = match store.get(&old_cst_complement_id)? {
        Object::CstComplement(c) => c,
        other => {
            return Err(VcsError::TypeMismatch {
                expected: "CstComplement".into(),
                got: other.type_name().into(),
            });
        }
    };

    // Re-store with updated data_id
    let new_comp = CstComplementObject {
        data_id: new_data_id,
        cst_complement: old_comp.cst_complement,
    };
    store.put(&Object::CstComplement(new_comp))
}

/// Store a CST complement alongside a data set in the VCS.
///
/// This is called during the initial ingest of data with format
/// preservation enabled. The complement captures the full CST Schema
/// for format-preserving emission later.
///
/// # Errors
///
/// Returns `VcsError` if serialization or storage fails.
pub fn store_cst_complement(
    store: &mut dyn Store,
    data_id: ObjectId,
    cst_complement_bytes: Vec<u8>,
) -> Result<ObjectId, VcsError> {
    let obj = CstComplementObject {
        data_id,
        cst_complement: cst_complement_bytes,
    };
    store.put(&Object::CstComplement(obj))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_store::MemStore;
    use crate::object::CommitObject;
    use panproto_inst::{Node, WInstance};
    use panproto_schema::Vertex;
    use std::collections::HashMap;

    fn make_schema(vertices: &[(&str, &str)]) -> Schema {
        let mut vert_map = HashMap::new();
        for (id, kind) in vertices {
            vert_map.insert(
                Name::from(*id),
                Vertex {
                    id: Name::from(*id),
                    kind: Name::from(*kind),
                    nsid: None,
                },
            );
        }
        Schema {
            protocol: "test".into(),
            vertices: vert_map,
            ..Default::default()
        }
    }

    /// The protocol the tests' schemas name, which no registry declares.
    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "record".into()],
            constraint_sorts: vec![],
            ..Default::default()
        }
    }

    /// The registered protocols the tests resolve "test" through.
    fn registered() -> HashMap<String, Protocol> {
        HashMap::from([("test".to_owned(), test_protocol())])
    }

    fn make_commit(schema_id: ObjectId, data_ids: Vec<ObjectId>) -> CommitObject {
        CommitObject::builder(schema_id, "test", "test", "test")
            .timestamp(0)
            .data_ids(data_ids)
            .build()
    }

    /// The schema `{a}` and its extension `{a, b}`, with the inclusion.
    fn schemas() -> (Schema, Schema, Migration) {
        let src = make_schema(&[("a", "object")]);
        let tgt = make_schema(&[("a", "object"), ("b", "string")]);
        let mapping = Migration::identity(&[Name::from("a")], &[]);
        (src, tgt, mapping)
    }

    /// A data set of one single-node record of `{a}`, under `schema_id`.
    fn one_record(store: &mut MemStore, schema_id: ObjectId) -> Result<ObjectId, VcsError> {
        let mut nodes = HashMap::new();
        nodes.insert(0_u32, Node::new(0, "a"));
        let inst = WInstance::new(nodes, vec![], vec![], 0, Name::from("a"));
        store.put(&Object::DataSet(DataSetObject::from_instance(
            schema_id,
            &Instance::Tree(vec![inst]),
            Some("rec-key".to_owned()),
        )?))
    }

    fn refusal<T: std::fmt::Debug>(result: Result<T, VcsError>) -> MigrationConflict {
        match result {
            Err(VcsError::Migration(conflict)) => *conflict,
            other => panic!("expected a migration conflict, got {other:?}"),
        }
    }

    #[test]
    fn staleness_compares_schema_identities() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let (old, new, _) = schemas();
        let old_tree = crate::tree::store_schema_as_tree(&mut store, old)?;
        let new_tree = crate::tree::store_schema_as_tree(&mut store, new.clone())?;
        let new_flat = store.put(&Object::FlatSchema(Box::new(new)))?;

        let stale_id = one_record(&mut store, old_tree)?;
        let current_id = one_record(&mut store, new_tree)?;
        let flat_id = one_record(&mut store, new_flat)?;
        let commit = make_commit(new_tree, vec![stale_id, current_id, flat_id]);

        let stale = detect_staleness(&store, &commit)?;
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].data_id, stale_id);
        assert_eq!(stale[0].data_schema_id, old_tree);
        assert_eq!(stale[0].head_schema_id, new_tree);
        // The flat schema is the commit's schema under another identifier.
        assert_eq!(schema_identity(&store, &new_flat)?, new_flat);
        assert_eq!(schema_identity(&store, &new_tree)?, new_flat);
        Ok(())
    }

    #[test]
    fn a_commit_without_data_has_no_stale_data() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let schema_id = crate::tree::store_schema_as_tree(&mut store, schemas().0)?;
        let stale = detect_staleness(&store, &make_commit(schema_id, vec![]))?;
        assert_eq!(stale.len(), 0);
        Ok(())
    }

    #[test]
    fn migrating_an_object_that_is_not_a_data_set_is_refused()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let (src, tgt, mapping) = schemas();
        let schema_id = crate::tree::store_schema_as_tree(&mut store, src.clone())?;
        let migration_id = store_migration(&mut store, &src, &tgt, mapping)?;
        let refused = refusal(migrate_forward(
            &mut store,
            schema_id,
            migration_id,
            &test_protocol(),
        ));
        assert_eq!(
            refused,
            MigrationConflict::WrongKind {
                id: schema_id,
                expected: "data set",
                found: "schema_tree",
            }
        );
        Ok(())
    }

    #[test]
    fn a_complement_names_its_stored_migration_and_migrated_data_its_schema_tree()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let (src, tgt, mapping) = schemas();
        let src_tree = crate::tree::store_schema_as_tree(&mut store, src.clone())?;
        let data_id = one_record(&mut store, src_tree)?;
        let migration_id = store_migration(&mut store, &src, &tgt, mapping)?;
        let (view_id, comp_id) =
            migrate_forward(&mut store, data_id, migration_id, &test_protocol())?;

        let Object::Complement(comp) = store.get(&comp_id)? else {
            panic!("expected a complement object");
        };
        assert_eq!(comp.migration_id, migration_id);
        assert_eq!(comp.data_id, data_id);
        assert_eq!(comp.migration(), Some(migration_id));
        assert!(matches!(
            store.get(&migration_id)?,
            Object::Migration { .. }
        ));

        let Object::DataSet(view) = store.get(&view_id)? else {
            panic!("expected a data set");
        };
        let mut expected = MemStore::new();
        assert_eq!(
            view.schema_id,
            crate::tree::store_schema_as_tree(&mut expected, tgt)?
        );
        assert_eq!(view.key.as_deref(), Some("rec-key"));

        // Backward restores the data set: the same schema tree and records.
        let restored_id = migrate_backward(&mut store, view_id, comp_id, &test_protocol())?;
        assert_eq!(restored_id, data_id);
        Ok(())
    }

    #[test]
    fn a_complement_whose_migration_is_its_data_set_names_no_migration()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let (src, tgt, mapping) = schemas();
        let src_tree = crate::tree::store_schema_as_tree(&mut store, src.clone())?;
        let data_id = one_record(&mut store, src_tree)?;
        let migration_id = store_migration(&mut store, &src, &tgt, mapping)?;
        let (view_id, comp_id) =
            migrate_forward(&mut store, data_id, migration_id, &test_protocol())?;
        let Object::Complement(mut comp) = store.get(&comp_id)? else {
            panic!("expected a complement object");
        };
        comp.migration_id = data_id;
        assert_eq!(comp.migration(), None);
        let legacy_id = store.put(&Object::Complement(comp))?;
        assert_eq!(
            refusal(migrate_backward(
                &mut store,
                view_id,
                legacy_id,
                &test_protocol()
            )),
            MigrationConflict::UnknownMigration {
                complement_id: legacy_id,
                data_id,
            }
        );
        Ok(())
    }

    #[test]
    fn infer_root_finds_vertex_without_incoming() {
        use panproto_schema::Edge;

        let mut schema = make_schema(&[("root", "object"), ("child", "string")]);

        let edge = Edge {
            src: "root".into(),
            tgt: "child".into(),
            kind: "prop".into(),
            name: Some("child".into()),
        };
        schema.edges.insert(edge.clone(), Name::from("prop"));
        schema
            .incoming
            .entry(Name::from("child"))
            .or_default()
            .push(edge);

        let root = infer_root(&schema);
        assert_eq!(root, "root");
    }

    #[test]
    fn a_protocol_name_resolves_in_order() -> Result<(), VcsError> {
        let schema = make_schema(&[("a", "object")]);
        let none = HashMap::new();
        let stored = Protocol {
            obj_kinds: vec!["stored".into()],
            ..test_protocol()
        };
        // A stored protocol of that name comes first.
        let found = protocol_for_schema(
            &schema,
            ProtocolSources {
                stored: Some(&stored),
                registered: &registered(),
            },
        )?;
        assert_eq!(found, stored);
        // A stored protocol of another name is passed over for the
        // registered one.
        let other = Protocol {
            name: "other".into(),
            ..test_protocol()
        };
        let found = protocol_for_schema(
            &schema,
            ProtocolSources {
                stored: Some(&other),
                registered: &registered(),
            },
        )?;
        assert_eq!(found, test_protocol());
        // A name only the registry declares resolves to its protocol.
        let atproto = resolve_protocol(
            "atproto",
            ProtocolSources {
                stored: None,
                registered: &none,
            },
        )?;
        assert_eq!(
            atproto,
            (panproto_protocols::registry::descriptor("atproto")
                .ok_or(VcsError::NothingStaged)?
                .protocol)()
        );
        Ok(())
    }

    #[test]
    fn data_migration_refuses_an_unknown_protocol() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let (src, tgt, mapping) = schemas();
        let src_tree = crate::tree::store_schema_as_tree(&mut store, src.clone())?;
        let ds_id = one_record(&mut store, src_tree)?;
        let commit = make_commit(src_tree, vec![ds_id]);
        match lift_commit_data(&mut store, &commit, (&src, &tgt), &mapping, &HashMap::new()) {
            Err(VcsError::UnknownProtocol { name }) => assert_eq!(name, "test"),
            other => panic!("an unknown protocol name was accepted: {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn data_migration_uses_the_resolved_protocol() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let (src, tgt, mapping) = schemas();
        let src_tree = crate::tree::store_schema_as_tree(&mut store, src.clone())?;
        let ds_id = one_record(&mut store, src_tree)?;
        // The commit records the protocol object, so it resolves the name
        // with nothing registered.
        let protocol_id = store.put(&Object::Protocol(Box::new(test_protocol())))?;
        let commit = CommitObject::builder(src_tree, "test", "test", "c")
            .timestamp(0)
            .data_ids(vec![ds_id])
            .protocol_id(protocol_id)
            .build();
        let (data_ids, complement_ids) =
            lift_commit_data(&mut store, &commit, (&src, &tgt), &mapping, &HashMap::new())?;
        assert_eq!(data_ids.len(), 1);
        assert_eq!(complement_ids.len(), 1);
        // The lift is the migration along the stored mapping under that
        // protocol.
        let migration_id = store_migration(&mut store, &src, &tgt, mapping)?;
        let expected = migrate_forward(&mut store, ds_id, migration_id, &test_protocol())?;
        assert_eq!((data_ids[0], complement_ids[0]), expected);
        Ok(())
    }

    #[test]
    fn lift_commit_data_lifts_through_schema_change() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let (src, tgt, mapping) = schemas();
        let src_tree = crate::tree::store_schema_as_tree(&mut store, src.clone())?;
        let ds_id = one_record(&mut store, src_tree)?;
        let commit = make_commit(src_tree, vec![ds_id]);

        let (data_ids, complement_ids) =
            lift_commit_data(&mut store, &commit, (&src, &tgt), &mapping, &registered())?;
        assert_eq!(data_ids.len(), 1);
        assert_eq!(complement_ids.len(), 1);
        assert_ne!(data_ids[0], ds_id, "data set lifted to a fresh object");

        match store.get(&data_ids[0])? {
            Object::DataSet(lifted) => {
                assert_eq!(lifted.record_count, 1);
                assert_eq!(lifted.key.as_deref(), Some("rec-key"));
                let insts: Vec<WInstance> = rmp_serde::from_slice(&lifted.data)?;
                assert_eq!(insts.len(), 1);
            }
            other => panic!("expected DataSet, got {}", other.type_name()),
        }
        Ok(())
    }

    #[test]
    fn a_stored_complement_is_decoded_whole_or_refused() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let (src, tgt, mapping) = schemas();
        let protocol = test_protocol();
        let src_tree = crate::tree::store_schema_as_tree(&mut store, src.clone())?;
        let ds_id = one_record(&mut store, src_tree)?;
        let migration_id = store_migration(&mut store, &src, &tgt, mapping)?;

        let (view_id, comp_id) = migrate_forward(&mut store, ds_id, migration_id, &protocol)?;
        migrate_backward(&mut store, view_id, comp_id, &protocol)?;

        // The same complement with a byte left over after it.
        let Object::Complement(mut comp) = store.get(&comp_id)? else {
            panic!("expected a complement object");
        };
        comp.complement.push(0);
        let padded_id = store.put(&Object::Complement(comp))?;
        match migrate_backward(&mut store, view_id, padded_id, &protocol) {
            Err(VcsError::UndecodablePayload { reason, .. }) => {
                assert_eq!(reason, "1 bytes follow the value");
            }
            other => panic!("a padded complement was accepted: {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn lift_commit_data_identity_schema_returns_ids_verbatim()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let (src, _, _) = schemas();
        let src_tree = crate::tree::store_schema_as_tree(&mut store, src.clone())?;
        let ds_id = one_record(&mut store, src_tree)?;
        let commit = make_commit(src_tree, vec![ds_id]);
        let before = store.list_objects()?.len();

        // Equal src and tgt: ids returned verbatim with no new objects.
        let (data_ids, complement_ids) = lift_commit_data(
            &mut store,
            &commit,
            (&src, &src),
            &Migration::identity(&[Name::from("a")], &[]),
            &HashMap::new(),
        )?;
        assert_eq!(data_ids, vec![ds_id]);
        assert_eq!(complement_ids, []);
        assert_eq!(store.list_objects()?.len(), before);
        Ok(())
    }
}
