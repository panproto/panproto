//! Three-way merge of the instances two commits track.
//!
//! A merge of two commits merges, key by key, the instances they track
//! ([`instance_vc::tracked`](crate::instance_vc::tracked)) against the
//! instances their merge base tracks. A key is decided at the level of data
//! sets first: a key both sides track under one data set, or neither tracks,
//! keeps that; a key one side left as the base had it takes the other side's
//! data set, or its absence. A key both sides changed is merged by deltas:
//! each side's instance is diffed against the base's
//! ([`instance_vc::diff`](crate::instance_vc::diff)), and the two deltas
//! merge when they are independent, as [`GraphDelta::merge`] and
//! [`TableDelta::merge`] define it; for a sequence of trees, record by
//! record. Every overlap is reported as a typed [`InstanceConflict`] and
//! never resolved by a policy: dependent deltas give one
//! [`InstanceConflict::Edit`] per conflict of their independence, and a
//! merged instance that fails validation gives the failures of the
//! validator's key checks as [`InstanceConflict::KeyConstraint`] and the
//! rest of its report as [`InstanceConflict::Validation`].
//!
//! The cases a delta merge does not cover are decided as follows, each
//! reported as an [`InstanceConflict::Tracking`]: a key the base does not
//! track and both sides track under different data sets has no common
//! instance to diff against ([`TrackingConflict::AddedOnBothSides`]); a
//! key one side stops tracking and the other changes is
//! [`TrackingConflict::RemovedAndChanged`]; base and sides of different
//! shapes are [`TrackingConflict::ShapeDiffers`]. Base and sides recorded
//! against different schemas are [`TrackingConflict::SchemaDiffers`] for
//! [`merge_tracked`]; [`merge_tracked_to`] instead migrates each of the three
//! to the merged schema ([`ToMerged`]) and merges the migrated instances, and
//! a version that does not migrate is an [`InstanceConflict::Migration`]
//! carrying the [`MigrationConflict`].
//!
//! For a sequence of trees the records the base holds are merged position by
//! position. A record both sides keep is merged through the graph forms of
//! its three versions; the merged graph must read back as a tree
//! ([`RecordConflict::NotATree`] otherwise). A record one side removes (a
//! tree delta removes a suffix) is removed when the other side leaves it as
//! the base had it, and is [`RecordConflict::RemovedAndChanged`] when the
//! other side changes it, or [`RecordConflict::RemovedByBoth`] when both
//! remove it, as a node both graph deltas delete is a conflict. Records
//! appended by one side follow the merged records; records appended by both
//! sides have no order between them and are
//! [`RecordConflict::AppendedByBoth`].
//!
//! Conflicts are listed in a deterministic order: by key, then record (none
//! first), then kind in the order of the variants of [`InstanceConflict`],
//! then the conflict's own order (a validation report's entries in report
//! order). [`apply_resolutions`] resolves them in the manner of
//! [`merge::apply_resolutions`](crate::merge::apply_resolutions): by index,
//! each choosing a side, all conflicts of one key the same side, which then
//! gives the key's data set (or its absence) whole.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use panproto_inst::graph_edit::EditConflict;
use panproto_inst::validation::{CheckKind, Diagnostic, Requirement};
use panproto_inst::{GInstance, GraphDelta, TableDelta, ValidationReport, WInstance};
use panproto_schema::{Protocol, Schema};

use crate::data_mig::MigrationConflict;
use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::instance_vc::graph_form;
use crate::merge::{ConflictResolution, ResolutionStrategy, Side};
use crate::object::{DataSetObject, InstanceShape};
use crate::payload::Instance;

/// The delta one side made to one record: a table's, a graph's, or a
/// tree record's graph form's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordDelta {
    /// A table delta.
    Table(TableDelta),
    /// A graph delta.
    Graph(GraphDelta),
}

/// A conflict over a whole key.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrackingConflict {
    /// The base does not track the key and the two sides track it under
    /// different data sets.
    AddedOnBothSides {
        /// Our data set.
        ours: ObjectId,
        /// Their data set.
        theirs: ObjectId,
    },
    /// One side stops tracking the key and the other changes its data set.
    RemovedAndChanged {
        /// The side that stops tracking it.
        removed_by: Side,
    },
    /// The base and the two sides hold instances of different shapes.
    ShapeDiffers {
        /// The base's shape.
        base: InstanceShape,
        /// Our shape.
        ours: InstanceShape,
        /// Their shape.
        theirs: InstanceShape,
    },
    /// The base and the two sides conform to different schemas.
    SchemaDiffers {
        /// The base's schema.
        base: ObjectId,
        /// Our schema.
        ours: ObjectId,
        /// Their schema.
        theirs: ObjectId,
    },
}

/// A conflict over one record of a sequence of trees.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecordConflict {
    /// Both sides remove the record.
    RemovedByBoth,
    /// One side removes the record and the other changes it.
    RemovedAndChanged {
        /// The side that removes it.
        removed_by: Side,
    },
    /// Both sides append records after the base's last.
    AppendedByBoth,
    /// The merged graph form of the record is not a tree.
    NotATree {
        /// Why it does not read back as a tree.
        reason: String,
    },
}

/// One of the three versions of a key a merge combines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Version {
    /// The merge base's (a replayed commit's first parent's).
    Base,
    /// Ours (the tip's, for a replay).
    Ours,
    /// Theirs (the replayed commit's).
    Theirs,
}

/// A conflict of an instance merge.
#[derive(Clone, Debug, PartialEq)]
pub enum InstanceConflict {
    /// A conflict over a whole key.
    Tracking {
        /// The key.
        key: String,
        /// The conflict.
        conflict: TrackingConflict,
    },
    /// One version of the key does not migrate to the merged schema.
    Migration {
        /// The key.
        key: String,
        /// The version.
        version: Version,
        /// Why it does not migrate.
        conflict: MigrationConflict,
    },
    /// The two sides' deltas of one record are not independent.
    Edit {
        /// The key.
        key: String,
        /// The tree record, for a sequence of trees.
        record: Option<usize>,
        /// Our delta from the base.
        ours: Box<RecordDelta>,
        /// Their delta from the base.
        theirs: Box<RecordDelta>,
        /// One conflict of the deltas' independence.
        conflict: EditConflict,
    },
    /// A conflict over one record of a sequence of trees.
    Record {
        /// The key.
        key: String,
        /// The record.
        record: usize,
        /// The conflict.
        conflict: RecordConflict,
    },
    /// The merged instance fails a key check of its validator
    /// ([`CheckKind::KeyUnique`] or [`CheckKind::ReferenceResolves`]).
    KeyConstraint {
        /// The key.
        key: String,
        /// The tree record, for a sequence of trees.
        record: Option<usize>,
        /// The required diagnostics of the key checks, in report order.
        diagnostics: Vec<Diagnostic>,
    },
    /// The merged instance fails another required check, or a required
    /// check does not complete.
    Validation {
        /// The key.
        key: String,
        /// The tree record, for a sequence of trees.
        record: Option<usize>,
        /// The report.
        report: Box<ValidationReport>,
    },
}

impl InstanceConflict {
    /// The key the conflict is under.
    #[must_use]
    pub fn key(&self) -> &str {
        match self {
            Self::Tracking { key, .. }
            | Self::Migration { key, .. }
            | Self::Edit { key, .. }
            | Self::Record { key, .. }
            | Self::KeyConstraint { key, .. }
            | Self::Validation { key, .. } => key,
        }
    }

    /// The record the conflict is about, for a sequence of trees.
    #[must_use]
    pub const fn record(&self) -> Option<usize> {
        match self {
            Self::Tracking { .. } | Self::Migration { .. } => None,
            Self::Record { record, .. } => Some(*record),
            Self::Edit { record, .. }
            | Self::KeyConstraint { record, .. }
            | Self::Validation { record, .. } => *record,
        }
    }

    const fn rank(&self) -> u8 {
        match self {
            Self::Tracking { .. } => 0,
            Self::Migration { .. } => 1,
            Self::Edit { .. } => 2,
            Self::Record { .. } => 3,
            Self::KeyConstraint { .. } => 4,
            Self::Validation { .. } => 5,
        }
    }

    /// The listing order: key, record, kind, then the conflict's own order;
    /// key-check and validation conflicts of one record compare equal, so a
    /// stable sort keeps them in report order.
    #[must_use]
    pub fn listing_order(&self, other: &Self) -> Ordering {
        self.key()
            .cmp(other.key())
            .then_with(|| self.record().cmp(&other.record()))
            .then_with(|| self.rank().cmp(&other.rank()))
            .then_with(|| match (self, other) {
                (Self::Tracking { conflict: a, .. }, Self::Tracking { conflict: b, .. }) => {
                    a.cmp(b)
                }
                (Self::Migration { version: a, .. }, Self::Migration { version: b, .. }) => {
                    a.cmp(b)
                }
                (Self::Edit { conflict: a, .. }, Self::Edit { conflict: b, .. }) => a.cmp(b),
                (Self::Record { conflict: a, .. }, Self::Record { conflict: b, .. }) => a.cmp(b),
                _ => Ordering::Equal,
            })
    }
}

/// What a merge gives a key.
#[derive(Clone, Debug)]
pub enum MergedEntry {
    /// A data set one of the commits already tracks under the key.
    Kept(ObjectId),
    /// A new instance, the merge of both sides' deltas, of the schema the
    /// base and both sides conform to, or of the merged schema when they were
    /// migrated to it.
    Merged {
        /// The schema.
        schema_id: ObjectId,
        /// The instance.
        instance: Box<Instance>,
    },
}

/// The data sets the two sides track under a conflicted key, by which a
/// resolution takes the key whole from one side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sides {
    /// Our data set, or `None` when we do not track the key.
    pub ours: Option<ObjectId>,
    /// Their data set, or `None` when they do not track the key.
    pub theirs: Option<ObjectId>,
}

/// The merge of the instances two commits track.
#[derive(Clone, Debug, Default)]
pub struct TrackedMerge {
    /// What the merge gives each key without a conflict that it tracks;
    /// a key without a conflict that is absent here is not tracked.
    pub merged: BTreeMap<String, MergedEntry>,
    /// The conflicts, in the listing order of
    /// [`InstanceConflict::listing_order`].
    pub conflicts: Vec<InstanceConflict>,
    /// For each conflicted key, the data sets the two sides track.
    pub sides: BTreeMap<String, Sides>,
}

/// A tracked map: the data set tracked under each key.
pub type Tracked = BTreeMap<String, (ObjectId, DataSetObject)>;

/// Reads a data set's schema and instance.
pub type LoadDataSet<'a> = dyn FnMut(&DataSetObject) -> Result<(Schema, Instance), VcsError> + 'a;

/// The failing records of an instance under a schema, each with its index
/// and report.
pub type ValidateInstance<'a> =
    dyn FnMut(&Schema, &Instance) -> Result<Vec<(usize, ValidationReport)>, VcsError> + 'a;

/// Migrates the data set a version of a key holds to the merged schema: its
/// instance there, or why it does not migrate.
pub type MigrateEntry<'a> = dyn FnMut(
        Version,
        ObjectId,
        &DataSetObject,
    ) -> Result<Result<Instance, MigrationConflict>, VcsError>
    + 'a;

/// The merged schema a merge migrates the versions of a key to when they
/// were recorded against different schemas.
pub struct ToMerged<'s, 'm> {
    /// The identifier merged instances are recorded under: the merged
    /// schema's schema tree.
    pub schema_id: ObjectId,
    /// The merged schema.
    pub schema: &'s Schema,
    /// The migration of each version (for instance
    /// [`MergeMigration::migrate`](crate::data_mig::MergeMigration::migrate)).
    pub migrate: &'s mut MigrateEntry<'m>,
}

impl std::fmt::Debug for ToMerged<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToMerged")
            .field("schema_id", &self.schema_id)
            .finish_non_exhaustive()
    }
}

/// What a key's three-way merge gives before validation.
enum KeyOutcome {
    Kept(Option<ObjectId>),
    Merged(Box<Instance>),
    Conflicted(Vec<InstanceConflict>),
}

/// Merge the instances `ours` and `theirs` track against those `base`
/// tracks.
///
/// `load` reads a data set's schema and instance; `validate` returns the
/// failing records of a merged instance under that schema, each with its
/// index and report, as
/// [`Repository::add_instance`](crate::Repository::add_instance) finds
/// them. Only a merged instance is validated: a data set taken from one
/// side is one that side committed.
///
/// # Errors
///
/// The errors of `load` and `validate`, and [`VcsError::DeltaDoesNotApply`]
/// when a tree has no graph form or the merge of independent deltas does
/// not apply to the base.
pub fn merge_tracked(
    base: &Tracked,
    ours: &Tracked,
    theirs: &Tracked,
    load: &mut LoadDataSet<'_>,
    validate: &mut ValidateInstance<'_>,
) -> Result<TrackedMerge, VcsError> {
    combine_tracked(Combine::Merge, [base, ours, theirs], load, validate, None)
}

/// Merge as [`merge_tracked`] does, except that a key whose three versions
/// are recorded against different schemas is not
/// [`TrackingConflict::SchemaDiffers`]: each version is migrated to the
/// merged schema by `to.migrate`, in the order base, ours, theirs, and the
/// migrated instances merge as three instances of the merged schema do. A
/// version that does not migrate is an [`InstanceConflict::Migration`]
/// carrying its [`MigrationConflict`], and the key is then not merged.
///
/// # Errors
///
/// As [`merge_tracked`], and the errors of `to.migrate`.
pub fn merge_tracked_to(
    base: &Tracked,
    ours: &Tracked,
    theirs: &Tracked,
    load: &mut LoadDataSet<'_>,
    validate: &mut ValidateInstance<'_>,
    to: &mut ToMerged<'_, '_>,
) -> Result<TrackedMerge, VcsError> {
    combine_tracked(
        Combine::Merge,
        [base, ours, theirs],
        load,
        validate,
        Some(to),
    )
}

/// How two deltas from one instance are combined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combine {
    /// Merged: the two deltas' merge applies to the base, and every
    /// conflict of their independence is reported.
    Merge,
    /// Rebased: the second side's delta is rebased over the first's
    /// ([`GraphDelta::rebase`], [`TableDelta::rebase`]) and applied to the
    /// first side's instance, and the [`RebaseConflict`](panproto_inst::graph_edit::RebaseConflict)'s
    /// conflict is reported.
    Rebase,
}

/// Replay a commit's changes to the instances it tracks onto a tip.
///
/// `parent` and `replayed` are the tracked maps of the replayed commit's
/// first parent and of the commit; `tip` is that of the commit it is
/// replayed onto. Only the keys the commit changes (those it tracks under
/// another data set than its parent, or stops tracking) are decided, as
/// [`merge_tracked`] decides them with the parent as base, the tip as ours
/// and the commit as theirs, except that two deltas of one instance are
/// combined by [`Combine::Rebase`]: the commit's delta is rebased over the
/// delta from the parent's instance to the tip's (the composite of the
/// deltas committed since) and applied to the tip's instance. The keys the
/// commit leaves as its parent had them are absent from the result.
///
/// # Errors
///
/// As [`merge_tracked`].
pub fn replay_tracked(
    parent: &Tracked,
    tip: &Tracked,
    replayed: &Tracked,
    load: &mut LoadDataSet<'_>,
    validate: &mut ValidateInstance<'_>,
) -> Result<TrackedMerge, VcsError> {
    replay_tracked_with(parent, tip, replayed, load, validate, None)
}

/// Replay as [`replay_tracked`] does, migrating the three versions of a
/// changed key recorded against different schemas to the replayed schema as
/// [`merge_tracked_to`] does.
///
/// # Errors
///
/// As [`merge_tracked_to`].
pub fn replay_tracked_to(
    parent: &Tracked,
    tip: &Tracked,
    replayed: &Tracked,
    load: &mut LoadDataSet<'_>,
    validate: &mut ValidateInstance<'_>,
    to: &mut ToMerged<'_, '_>,
) -> Result<TrackedMerge, VcsError> {
    replay_tracked_with(parent, tip, replayed, load, validate, Some(to))
}

fn replay_tracked_with(
    parent: &Tracked,
    tip: &Tracked,
    replayed: &Tracked,
    load: &mut LoadDataSet<'_>,
    validate: &mut ValidateInstance<'_>,
    to: Option<&mut ToMerged<'_, '_>>,
) -> Result<TrackedMerge, VcsError> {
    let changed: BTreeSet<&String> = parent
        .keys()
        .chain(replayed.keys())
        .filter(|key| !same(parent.get(*key), replayed.get(*key)))
        .collect();
    let only = |map: &Tracked| -> Tracked {
        map.iter()
            .filter(|(key, _)| changed.contains(key))
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect()
    };
    combine_tracked(
        Combine::Rebase,
        [&only(parent), &only(tip), &only(replayed)],
        load,
        validate,
        to,
    )
}

fn combine_tracked(
    mode: Combine,
    [base, ours, theirs]: [&Tracked; 3],
    load: &mut LoadDataSet<'_>,
    validate: &mut ValidateInstance<'_>,
    mut to: Option<&mut ToMerged<'_, '_>>,
) -> Result<TrackedMerge, VcsError> {
    let keys: BTreeSet<&String> = base
        .keys()
        .chain(ours.keys())
        .chain(theirs.keys())
        .collect();
    let mut out = TrackedMerge::default();
    for key in keys {
        let entries = [base.get(key), ours.get(key), theirs.get(key)];
        let (outcome, schema) = merge_key(mode, key, entries, load, to.as_deref_mut())?;
        let conflicts = match outcome {
            KeyOutcome::Kept(None) => Vec::new(),
            KeyOutcome::Kept(Some(id)) => {
                out.merged.insert(key.clone(), MergedEntry::Kept(id));
                Vec::new()
            }
            KeyOutcome::Merged(instance) => {
                let Some((schema_id, schema)) = schema else {
                    return Err(VcsError::DeltaDoesNotApply {
                        reason: format!("the merge of {key:?} has no schema"),
                    });
                };
                let failing = validate(&schema, &instance)?;
                let conflicts = validation_conflicts(key, &instance, failing);
                if conflicts.is_empty() {
                    out.merged.insert(
                        key.clone(),
                        MergedEntry::Merged {
                            schema_id,
                            instance,
                        },
                    );
                }
                conflicts
            }
            KeyOutcome::Conflicted(conflicts) => conflicts,
        };
        if !conflicts.is_empty() {
            out.sides.insert(
                key.clone(),
                Sides {
                    ours: entries[1].map(|(id, _)| *id),
                    theirs: entries[2].map(|(id, _)| *id),
                },
            );
            out.conflicts.extend(conflicts);
        }
    }
    out.conflicts.sort_by(InstanceConflict::listing_order);
    Ok(out)
}

/// The failures of a merged instance's validation as conflicts.
fn validation_conflicts(
    key: &str,
    instance: &Instance,
    failing: Vec<(usize, ValidationReport)>,
) -> Vec<InstanceConflict> {
    let is_tree = matches!(instance, Instance::Tree(_));
    let required = Requirement::Required;
    let mut out = Vec::new();
    for (index, report) in failing {
        let record = is_tree.then_some(index);
        let keyed: Vec<Diagnostic> = report
            .diagnostics
            .iter()
            .filter(|d| d.requirement == required && is_key_check(d.check.kind))
            .cloned()
            .collect();
        let other = report
            .diagnostics
            .iter()
            .any(|d| d.requirement == required && !is_key_check(d.check.kind))
            || report.incomplete.iter().any(|i| i.requirement == required);
        if !keyed.is_empty() {
            out.push(InstanceConflict::KeyConstraint {
                key: key.to_owned(),
                record,
                diagnostics: keyed,
            });
        }
        if other {
            out.push(InstanceConflict::Validation {
                key: key.to_owned(),
                record,
                report: Box::new(report),
            });
        }
    }
    out
}

/// Whether a check is one of the validator's key checks.
const fn is_key_check(kind: CheckKind) -> bool {
    matches!(kind, CheckKind::KeyUnique | CheckKind::ReferenceResolves)
}

type Entry<'a> = Option<&'a (ObjectId, DataSetObject)>;

fn same(a: Entry<'_>, b: Entry<'_>) -> bool {
    a.map(|(id, _)| id) == b.map(|(id, _)| id)
}

/// A key's merge, with the schema of a merged instance.
fn merge_key(
    mode: Combine,
    key: &str,
    [base, ours, theirs]: [Entry<'_>; 3],
    load: &mut LoadDataSet<'_>,
    to: Option<&mut ToMerged<'_, '_>>,
) -> Result<(KeyOutcome, Option<(ObjectId, Schema)>), VcsError> {
    let kept = |entry: Entry<'_>| KeyOutcome::Kept(entry.map(|(id, _)| *id));
    if same(ours, theirs) || same(theirs, base) {
        return Ok((kept(ours), None));
    }
    if same(ours, base) {
        return Ok((kept(theirs), None));
    }
    let tracking = |conflict| {
        KeyOutcome::Conflicted(vec![InstanceConflict::Tracking {
            key: key.to_owned(),
            conflict,
        }])
    };
    let (Some(base), Some(ours), Some(theirs)) = (base, ours, theirs) else {
        let conflict = match (base, ours, theirs) {
            (None, Some((o, _)), Some((t, _))) => TrackingConflict::AddedOnBothSides {
                ours: *o,
                theirs: *t,
            },
            (_, None, _) => TrackingConflict::RemovedAndChanged {
                removed_by: Side::Ours,
            },
            _ => TrackingConflict::RemovedAndChanged {
                removed_by: Side::Theirs,
            },
        };
        return Ok((tracking(conflict), None));
    };
    let shapes = [base, ours, theirs].map(|(_, data)| data.encoding.shape);
    if shapes[0] != shapes[1] || shapes[0] != shapes[2] {
        let conflict = TrackingConflict::ShapeDiffers {
            base: shapes[0],
            ours: shapes[1],
            theirs: shapes[2],
        };
        return Ok((tracking(conflict), None));
    }
    let schemas = [base, ours, theirs].map(|(_, data)| data.schema_id);
    if schemas[0] != schemas[1] || schemas[0] != schemas[2] {
        if let Some(to) = to {
            return migrate_and_combine(mode, key, [base, ours, theirs], to);
        }
        let conflict = TrackingConflict::SchemaDiffers {
            base: schemas[0],
            ours: schemas[1],
            theirs: schemas[2],
        };
        return Ok((tracking(conflict), None));
    }
    let (schema, base_instance) = load(&base.1)?;
    let (_, ours_instance) = load(&ours.1)?;
    let (_, theirs_instance) = load(&theirs.1)?;
    let outcome = combine_instances(
        mode,
        key,
        &schema,
        &base_instance,
        &ours_instance,
        &theirs_instance,
    )?;
    Ok((outcome_of(outcome), Some((schemas[0], schema))))
}

/// Migrate the three versions of a key to the merged schema and combine
/// them, or report every version that does not migrate.
fn migrate_and_combine(
    mode: Combine,
    key: &str,
    entries: [&(ObjectId, DataSetObject); 3],
    to: &mut ToMerged<'_, '_>,
) -> Result<(KeyOutcome, Option<(ObjectId, Schema)>), VcsError> {
    let mut migrated = Vec::with_capacity(3);
    let mut conflicts = Vec::new();
    for (version, (id, dataset)) in [Version::Base, Version::Ours, Version::Theirs]
        .into_iter()
        .zip(entries)
    {
        match (to.migrate)(version, *id, dataset)? {
            Ok(instance) => migrated.push(instance),
            Err(conflict) => conflicts.push(InstanceConflict::Migration {
                key: key.to_owned(),
                version,
                conflict,
            }),
        }
    }
    if !conflicts.is_empty() {
        return Ok((KeyOutcome::Conflicted(conflicts), None));
    }
    let [base, ours, theirs] = [&migrated[0], &migrated[1], &migrated[2]];
    let outcome = combine_instances(mode, key, to.schema, base, ours, theirs)?;
    Ok((outcome_of(outcome), Some((to.schema_id, to.schema.clone()))))
}

fn outcome_of(result: Result<Instance, Vec<InstanceConflict>>) -> KeyOutcome {
    match result {
        Ok(instance) => KeyOutcome::Merged(Box::new(instance)),
        Err(conflicts) => KeyOutcome::Conflicted(conflicts),
    }
}

/// The three-way merge of three instances of one shape of `schema` under
/// `key`: the merged instance, or the conflicts in listing order.
///
/// # Errors
///
/// [`VcsError::ShapeMismatch`] when the three are not of one shape, and
/// [`VcsError::DeltaDoesNotApply`] when a tree has no graph form or the
/// merge of independent deltas does not apply to the base.
pub fn merge_instances(
    key: &str,
    schema: &Schema,
    base: &Instance,
    ours: &Instance,
    theirs: &Instance,
) -> Result<Result<Instance, Vec<InstanceConflict>>, VcsError> {
    combine_instances(Combine::Merge, key, schema, base, ours, theirs)
}

/// The replay onto `tip` of the change from `parent` to `replayed`.
///
/// The three are instances of one shape of `schema` under `key`. The delta
/// from `parent` to `replayed` is rebased over the delta from `parent` to
/// `tip` and applied to `tip`, record by record for a sequence of trees
/// under the rules of [`merge_instances`]; a refused rebase gives its one
/// conflict per conflicting record.
///
/// # Errors
///
/// As [`merge_instances`].
pub fn rebase_instances(
    key: &str,
    schema: &Schema,
    parent: &Instance,
    tip: &Instance,
    replayed: &Instance,
) -> Result<Result<Instance, Vec<InstanceConflict>>, VcsError> {
    combine_instances(Combine::Rebase, key, schema, parent, tip, replayed)
}

fn combine_instances(
    mode: Combine,
    key: &str,
    schema: &Schema,
    base: &Instance,
    ours: &Instance,
    theirs: &Instance,
) -> Result<Result<Instance, Vec<InstanceConflict>>, VcsError> {
    match (base, ours, theirs) {
        (Instance::Graph(b), Instance::Graph(o), Instance::Graph(t)) => {
            merge_graphs(mode, key, None, b, o, t).map(|merged| merged.map(Instance::Graph))
        }
        (Instance::Table(b), Instance::Table(o), Instance::Table(t)) => {
            let (dours, dtheirs) = (TableDelta::diff(b, o), TableDelta::diff(b, t));
            let combined = match mode {
                Combine::Merge => {
                    let conflicts = dours.independence(&dtheirs);
                    if conflicts.is_empty() {
                        let merged = dours.merge(&dtheirs).map_err(|e| refused(&e))?;
                        Ok((merged, b))
                    } else {
                        Err(conflicts)
                    }
                }
                Combine::Rebase => dtheirs
                    .rebase(std::slice::from_ref(&dours))
                    .map(|rebased| (rebased, o))
                    .map_err(|found| vec![found.conflict]),
            };
            match combined {
                Ok((delta, onto)) => {
                    let mut out = onto.clone();
                    delta.apply(&mut out).map_err(|e| refused(&e))?;
                    Ok(Ok(Instance::Table(out)))
                }
                Err(conflicts) => Ok(Err(edit_conflicts(
                    key,
                    None,
                    &RecordDelta::Table(dours),
                    &RecordDelta::Table(dtheirs),
                    conflicts,
                ))),
            }
        }
        (Instance::Tree(b), Instance::Tree(o), Instance::Tree(t)) => {
            merge_trees(mode, key, schema, b, o, t)
        }
        _ => {
            let (expected, found) = if base.shape() == ours.shape() {
                (base.shape(), theirs.shape())
            } else {
                (base.shape(), ours.shape())
            };
            Err(VcsError::ShapeMismatch { expected, found })
        }
    }
}

fn refused(error: &dyn std::fmt::Display) -> VcsError {
    VcsError::DeltaDoesNotApply {
        reason: error.to_string(),
    }
}

fn edit_conflicts(
    key: &str,
    record: Option<usize>,
    ours: &RecordDelta,
    theirs: &RecordDelta,
    conflicts: Vec<EditConflict>,
) -> Vec<InstanceConflict> {
    conflicts
        .into_iter()
        .map(|conflict| InstanceConflict::Edit {
            key: key.to_owned(),
            record,
            ours: Box::new(ours.clone()),
            theirs: Box::new(theirs.clone()),
            conflict,
        })
        .collect()
}

/// The three-way merge, or rebase, of three graphs.
fn merge_graphs(
    mode: Combine,
    key: &str,
    record: Option<usize>,
    base: &GInstance,
    ours: &GInstance,
    theirs: &GInstance,
) -> Result<Result<GInstance, Vec<InstanceConflict>>, VcsError> {
    let (dours, dtheirs) = (GraphDelta::diff(base, ours), GraphDelta::diff(base, theirs));
    let combined = match mode {
        Combine::Merge => {
            let conflicts = dours.independence(&dtheirs);
            if conflicts.is_empty() {
                let merged = dours.merge(&dtheirs).map_err(|e| refused(&e))?;
                Ok((merged, base))
            } else {
                Err(conflicts)
            }
        }
        Combine::Rebase => dtheirs
            .rebase(std::slice::from_ref(&dours))
            .map(|rebased| (rebased, ours))
            .map_err(|found| vec![found.conflict]),
    };
    let conflicts = match combined {
        Ok((delta, onto)) => {
            let mut out = onto.clone();
            delta.apply(&mut out).map_err(|e| refused(&e))?;
            return Ok(Ok(out));
        }
        Err(conflicts) => conflicts,
    };
    Ok(Err(edit_conflicts(
        key,
        record,
        &RecordDelta::Graph(dours),
        &RecordDelta::Graph(dtheirs),
        conflicts,
    )))
}

/// The three-way merge of three sequences of trees, record by record.
fn merge_trees(
    mode: Combine,
    key: &str,
    schema: &Schema,
    base: &[WInstance],
    ours: &[WInstance],
    theirs: &[WInstance],
) -> Result<Result<Instance, Vec<InstanceConflict>>, VcsError> {
    let mut conflicts = Vec::new();
    let mut merged = Vec::new();
    let record_conflict = |record, conflict| InstanceConflict::Record {
        key: key.to_owned(),
        record,
        conflict,
    };
    for (index, tree) in base.iter().enumerate() {
        let base_graph = graph_form(schema, tree)?;
        match (ours.get(index), theirs.get(index)) {
            (Some(o), Some(t)) => {
                let (o, t) = (graph_form(schema, o)?, graph_form(schema, t)?);
                match merge_graphs(mode, key, Some(index), &base_graph, &o, &t)? {
                    Ok(graph) => match graph.to_wtype_embedded(schema) {
                        Ok((tree, _)) => merged.push(tree),
                        Err(error) => conflicts.push(record_conflict(
                            index,
                            RecordConflict::NotATree {
                                reason: error.to_string(),
                            },
                        )),
                    },
                    Err(found) => conflicts.extend(found),
                }
            }
            (None, None) => conflicts.push(record_conflict(index, RecordConflict::RemovedByBoth)),
            (None, Some(kept)) | (Some(kept), None) => {
                let changed =
                    !GraphDelta::diff(&base_graph, &graph_form(schema, kept)?).is_identity();
                if changed {
                    let removed_by = if ours.get(index).is_none() {
                        Side::Ours
                    } else {
                        Side::Theirs
                    };
                    conflicts.push(record_conflict(
                        index,
                        RecordConflict::RemovedAndChanged { removed_by },
                    ));
                }
            }
        }
    }
    let appended_ours = ours.get(base.len()..).unwrap_or_default();
    let appended_theirs = theirs.get(base.len()..).unwrap_or_default();
    if !appended_ours.is_empty() && !appended_theirs.is_empty() {
        conflicts.push(record_conflict(base.len(), RecordConflict::AppendedByBoth));
    }
    if !conflicts.is_empty() {
        conflicts.sort_by(InstanceConflict::listing_order);
        return Ok(Err(conflicts));
    }
    merged.extend(appended_ours.iter().chain(appended_theirs).cloned());
    Ok(Ok(Instance::Tree(merged)))
}

/// The entries a merge resolved by `strategy` gives each key.
///
/// They are the merged entries and, for each conflicted key, the data set
/// of the side its conflicts are resolved for (the key is absent when that
/// side does not track it).
///
/// # Errors
///
/// [`VcsError::UnresolvedInstanceConflicts`] when a conflict has no
/// resolution, and [`VcsError::InconsistentInstanceResolution`] naming the
/// first key whose conflicts are resolved for different sides.
pub fn apply_resolutions(
    merge: &TrackedMerge,
    strategy: &ResolutionStrategy,
) -> Result<BTreeMap<String, MergedEntry>, VcsError> {
    let total = merge.conflicts.len();
    let unresolved = (0..total)
        .filter(|index| !strategy.resolutions.contains_key(index))
        .count();
    if unresolved > 0 {
        return Err(VcsError::UnresolvedInstanceConflicts {
            count: unresolved,
            total,
        });
    }
    let mut chosen: BTreeMap<&str, Side> = BTreeMap::new();
    for (index, conflict) in merge.conflicts.iter().enumerate() {
        let side = match strategy.resolutions.get(&index) {
            Some(ConflictResolution::ChooseOurs) => Side::Ours,
            Some(ConflictResolution::ChooseTheirs) => Side::Theirs,
            None => {
                return Err(VcsError::UnresolvedInstanceConflicts { count: 1, total });
            }
        };
        if *chosen.entry(conflict.key()).or_insert_with(|| side.clone()) != side {
            return Err(VcsError::InconsistentInstanceResolution {
                key: conflict.key().to_owned(),
            });
        }
    }
    let mut out = merge.merged.clone();
    for (key, side) in chosen {
        let sides = merge.sides.get(key).copied().unwrap_or(Sides {
            ours: None,
            theirs: None,
        });
        let id = match side {
            Side::Ours => sides.ours,
            Side::Theirs => sides.theirs,
        };
        if let Some(id) = id {
            out.insert(key.to_owned(), MergedEntry::Kept(id));
        }
    }
    Ok(out)
}

/// The failing records of `instance` under `schema` and `protocol`: each
/// record (tree, or the one table or graph) whose report has a required
/// check that failed or did not complete, with its index.
pub(crate) fn instance_failures(
    schema: &Schema,
    protocol: &Protocol,
    instance: &crate::payload::Instance,
) -> Vec<(usize, panproto_inst::ValidationReport)> {
    let reports = match instance {
        Instance::Tree(trees) => trees
            .iter()
            .map(|tree| panproto_inst::validate_wtype(tree, schema, protocol))
            .collect(),
        Instance::Table(table) => vec![panproto_inst::validate_table(table, schema, protocol)],
        Instance::Graph(graph) => vec![panproto_inst::validate_graph(graph, schema, protocol)],
    };
    let required = panproto_inst::validation::Requirement::Required;
    reports
        .into_iter()
        .enumerate()
        .filter(|(_, report)| {
            report.diagnostics.iter().any(|d| d.requirement == required)
                || report.incomplete.iter().any(|i| i.requirement == required)
        })
        .collect()
}

/// What a replayed commit records about the instances it changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayedData {
    /// One data set per key the replay changes and tracks, in key order.
    pub data_ids: Vec<ObjectId>,
    /// The keys the replay stops tracking.
    pub untracked: Vec<String>,
}

/// The replayed schema of a replay and the replay's migrations to it.
#[derive(Clone, Copy, Debug)]
pub struct ReplayTarget<'a> {
    /// The tip's identifier.
    pub tip: ObjectId,
    /// The replayed commit's identifier.
    pub commit: ObjectId,
    /// The tip's schema.
    pub tip_schema: &'a Schema,
    /// The replayed commit's schema.
    pub commit_schema: &'a Schema,
    /// The replayed schema: the merge of the tip's and the commit's
    /// schemas over the commit's first parent's.
    pub merged: &'a Schema,
    /// The migration from the tip's schema to the replayed schema.
    pub from_tip: &'a panproto_mig::Migration,
    /// The migration from the commit's schema to the replayed schema.
    pub from_commit: &'a panproto_mig::Migration,
}

/// Replay onto `tip` the instances `commit` changes from its first parent
/// `parent` ([`replay_tracked_to`]), and store what the replayed commit
/// records.
///
/// `lifted` lists, aligned with `commit.data_ids`, each of the commit's data
/// sets as lifted to the replayed commit's schema; a data set the replay
/// takes from the commit is recorded in its lifted form. A merged instance
/// is stored under the schema its three versions share, or, when they were
/// recorded against different schemas and each migrated to the replayed
/// schema (along first-parent history and `target`'s migrations,
/// [`MergeMigration`](crate::data_mig::MergeMigration) with the tip as our
/// side and the commit as theirs), under the replayed schema's schema tree.
/// Merged instances are validated under the protocol their schema names,
/// resolved through the commit's protocol object, then `registered`, then
/// the registry.
///
/// # Errors
///
/// [`VcsError::InstanceConflicts`] with every conflict when the replay
/// conflicts, and the errors of [`replay_tracked_to`], of tracking and of
/// the store.
pub fn replay_commit_data<S: std::hash::BuildHasher>(
    store: &mut dyn crate::store::Store,
    [parent, tip, commit]: [&crate::object::CommitObject; 3],
    target: &ReplayTarget<'_>,
    lifted: &[ObjectId],
    registered: &std::collections::HashMap<String, Protocol, S>,
) -> Result<ReplayedData, VcsError> {
    let parent_tracked = crate::instance_vc::tracked(store, parent)?;
    let tip_tracked = crate::instance_vc::tracked(store, tip)?;
    let replayed = crate::instance_vc::tracked(store, commit)?;
    let stored = crate::data_mig::commit_protocol(store, commit)?;
    let merged_identity = crate::hash::hash_schema(target.merged)?;
    let merged_tree = crate::tree::store_schema_as_tree(store, target.merged.clone())?;
    let mut side = |id: ObjectId,
                    side_commit: &crate::object::CommitObject,
                    schema: &Schema,
                    mapping: &panproto_mig::Migration|
     -> Result<crate::data_mig::MergeSide, VcsError> {
        let migration = if crate::hash::hash_schema(schema)? == merged_identity {
            None
        } else {
            Some(crate::data_mig::store_migration(
                store,
                schema,
                target.merged,
                mapping.clone(),
            )?)
        };
        Ok(crate::data_mig::MergeSide {
            commit: id,
            migration,
            protocol: crate::data_mig::commit_protocol(store, side_commit)?,
        })
    };
    let sides = [
        side(target.tip, tip, target.tip_schema, target.from_tip)?,
        side(
            target.commit,
            commit,
            target.commit_schema,
            target.from_commit,
        )?,
    ];
    let replay = {
        let view: &dyn crate::store::Store = &*store;
        let migration = crate::data_mig::MergeMigration {
            sides,
            merged: merged_identity,
            registered,
        };
        let mut migrate =
            |version, id, dataset: &DataSetObject| migration.migrate(view, version, id, dataset);
        let mut to = ToMerged {
            schema_id: merged_tree,
            schema: target.merged,
            migrate: &mut migrate,
        };
        let coproduct = crate::tree::project_coproduct_protocol();
        let mut load = |dataset: &DataSetObject| {
            Ok((
                crate::tree::assemble_schema_dyn(view, &dataset.schema_id, &coproduct)?,
                dataset.decode()?,
            ))
        };
        let mut validate = |schema: &Schema, instance: &Instance| {
            let protocol = crate::data_mig::protocol_for_schema(
                schema,
                crate::data_mig::ProtocolSources {
                    stored: stored.as_ref(),
                    registered,
                },
            )?;
            Ok(instance_failures(schema, &protocol, instance))
        };
        replay_tracked_to(
            &parent_tracked,
            &tip_tracked,
            &replayed,
            &mut load,
            &mut validate,
            &mut to,
        )?
    };
    if !replay.conflicts.is_empty() {
        return Err(VcsError::InstanceConflicts {
            conflicts: replay.conflicts,
        });
    }
    let changed: BTreeSet<&String> = parent_tracked
        .keys()
        .chain(replayed.keys())
        .filter(|key| !same(parent_tracked.get(*key), replayed.get(*key)))
        .collect();
    let mut out = ReplayedData::default();
    for key in changed {
        match replay.merged.get(key) {
            None => {
                if tip_tracked.contains_key(key) {
                    out.untracked.push(key.clone());
                }
            }
            Some(MergedEntry::Kept(id)) => {
                let recorded = commit
                    .data_ids
                    .iter()
                    .position(|own| own == id)
                    .and_then(|index| lifted.get(index))
                    .unwrap_or(id);
                if tip_tracked.get(key).map(|(tip_id, _)| tip_id) != Some(recorded) {
                    out.data_ids.push(*recorded);
                }
            }
            Some(MergedEntry::Merged {
                schema_id,
                instance,
            }) => {
                let dataset =
                    DataSetObject::from_instance(*schema_id, instance, Some(key.clone()))?;
                out.data_ids
                    .push(store.put(&crate::object::Object::DataSet(dataset))?);
            }
        }
    }
    Ok(out)
}
