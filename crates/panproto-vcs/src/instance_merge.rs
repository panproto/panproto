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
//! shapes are [`TrackingConflict::ShapeDiffers`], and of different schemas
//! [`TrackingConflict::SchemaDiffers`] (an instance is migrated before it is
//! merged).
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
use panproto_schema::Schema;

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
            Self::Tracking { .. } => None,
            Self::Record { record, .. } => Some(*record),
            Self::Edit { record, .. }
            | Self::KeyConstraint { record, .. }
            | Self::Validation { record, .. } => *record,
        }
    }

    const fn rank(&self) -> u8 {
        match self {
            Self::Tracking { .. } => 0,
            Self::Edit { .. } => 1,
            Self::Record { .. } => 2,
            Self::KeyConstraint { .. } => 3,
            Self::Validation { .. } => 4,
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
    /// base and both sides conform to.
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
    let keys: BTreeSet<&String> = base
        .keys()
        .chain(ours.keys())
        .chain(theirs.keys())
        .collect();
    let mut out = TrackedMerge::default();
    for key in keys {
        let entries = [base.get(key), ours.get(key), theirs.get(key)];
        let (outcome, schema) = merge_key(key, entries, load)?;
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
    key: &str,
    [base, ours, theirs]: [Entry<'_>; 3],
    load: &mut LoadDataSet<'_>,
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
    let outcome = merge_instances(
        key,
        &schema,
        &base_instance,
        &ours_instance,
        &theirs_instance,
    )?;
    Ok((outcome_of(outcome), Some((schemas[0], schema))))
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
    match (base, ours, theirs) {
        (Instance::Graph(b), Instance::Graph(o), Instance::Graph(t)) => {
            merge_graphs(key, None, b, o, t).map(|merged| merged.map(Instance::Graph))
        }
        (Instance::Table(b), Instance::Table(o), Instance::Table(t)) => {
            let (dours, dtheirs) = (TableDelta::diff(b, o), TableDelta::diff(b, t));
            let conflicts = dours.independence(&dtheirs);
            if conflicts.is_empty() {
                let merged = dours.merge(&dtheirs).map_err(|e| refused(&e))?;
                let mut out = b.clone();
                merged.apply(&mut out).map_err(|e| refused(&e))?;
                return Ok(Ok(Instance::Table(out)));
            }
            Ok(Err(edit_conflicts(
                key,
                None,
                &RecordDelta::Table(dours),
                &RecordDelta::Table(dtheirs),
                conflicts,
            )))
        }
        (Instance::Tree(b), Instance::Tree(o), Instance::Tree(t)) => {
            merge_trees(key, schema, b, o, t)
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

/// The three-way merge of three graphs.
fn merge_graphs(
    key: &str,
    record: Option<usize>,
    base: &GInstance,
    ours: &GInstance,
    theirs: &GInstance,
) -> Result<Result<GInstance, Vec<InstanceConflict>>, VcsError> {
    let (dours, dtheirs) = (GraphDelta::diff(base, ours), GraphDelta::diff(base, theirs));
    let conflicts = dours.independence(&dtheirs);
    if conflicts.is_empty() {
        let merged = dours.merge(&dtheirs).map_err(|e| refused(&e))?;
        let mut out = base.clone();
        merged.apply(&mut out).map_err(|e| refused(&e))?;
        return Ok(Ok(out));
    }
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
                match merge_graphs(key, Some(index), &base_graph, &o, &t)? {
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
