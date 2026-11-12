//! Deltas between committed instances and the instances a commit tracks.
//!
//! A commit records the data sets staged for it and the keys it stops
//! tracking. The instance it tracks under a key is the data set with that key
//! it records; none when it stops tracking the key; or else the one its first
//! parent tracks under the key: [`tracked`] reads that map. A key once
//! tracked stays tracked along first-parent history until a commit records
//! another data set under it or stops tracking it. The difference between two instances
//! of one shape is the shape's delta: [`TableDelta::diff`] for tables,
//! [`GraphDelta::diff`] for graphs, and for a sequence of trees the graph
//! delta between the graph forms of each record both sequences hold, with
//! the records only one of them holds. [`apply`] applies such a delta.

use std::collections::BTreeMap;

use panproto_inst::{GInstance, GraphDelta, TableDelta, WInstance};
use panproto_schema::Schema;

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{CommitObject, DataSetObject, InstanceShape, Object};
use crate::payload::Instance;
use crate::store::Store;

/// The delta between two sequences of trees.
#[derive(Clone, Debug)]
pub struct TreeRecordsDelta {
    /// For each record both sequences hold, in order, the graph delta from
    /// the first record's graph form to the second's.
    pub records: Vec<GraphDelta>,
    /// The records only the first sequence holds: its suffix past the
    /// second's length.
    pub removed: Vec<WInstance>,
    /// The records only the second sequence holds: its suffix past the
    /// first's length.
    pub appended: Vec<WInstance>,
}

/// The delta between two instances of one shape.
#[derive(Clone, Debug)]
pub enum InstanceDelta {
    /// Between two sequences of trees.
    Tree(TreeRecordsDelta),
    /// Between two table instances.
    Table(TableDelta),
    /// Between two graph instances.
    Graph(GraphDelta),
}

impl InstanceDelta {
    /// The shape of the instances the delta is between.
    #[must_use]
    pub const fn shape(&self) -> InstanceShape {
        match self {
            Self::Tree(_) => InstanceShape::Tree,
            Self::Table(_) => InstanceShape::Table,
            Self::Graph(_) => InstanceShape::Graph,
        }
    }

    /// Whether the delta changes nothing.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        match self {
            Self::Tree(delta) => {
                delta.removed.is_empty()
                    && delta.appended.is_empty()
                    && delta.records.iter().all(GraphDelta::is_identity)
            }
            Self::Table(delta) => delta.is_identity(),
            Self::Graph(delta) => delta.is_identity(),
        }
    }
}

/// The graph form of a tree under `schema`.
pub(crate) fn graph_form(schema: &Schema, tree: &WInstance) -> Result<GInstance, VcsError> {
    GInstance::from_wtype_embedded(tree, Some(schema))
        .map(|(graph, _)| graph)
        .map_err(|error| VcsError::DeltaDoesNotApply {
            reason: format!("a tree has no graph form: {error}"),
        })
}

/// The delta from `before` to `after`, two instances of one shape of the
/// schema `schema`.
///
/// # Errors
///
/// [`VcsError::ShapeMismatch`] when the two instances have different
/// shapes, and [`VcsError::DeltaDoesNotApply`] when a tree has no graph
/// form (a node filed under another id, a second parent, reserved fan
/// vocabulary or exhausted ids).
pub fn diff(
    schema: &Schema,
    before: &Instance,
    after: &Instance,
) -> Result<InstanceDelta, VcsError> {
    match (before, after) {
        (Instance::Table(a), Instance::Table(b)) => {
            Ok(InstanceDelta::Table(TableDelta::diff(a, b)))
        }
        (Instance::Graph(a), Instance::Graph(b)) => {
            Ok(InstanceDelta::Graph(GraphDelta::diff(a, b)))
        }
        (Instance::Tree(a), Instance::Tree(b)) => {
            let common = a.len().min(b.len());
            let records = a
                .iter()
                .zip(b)
                .map(|(x, y)| {
                    Ok(GraphDelta::diff(
                        &graph_form(schema, x)?,
                        &graph_form(schema, y)?,
                    ))
                })
                .collect::<Result<Vec<_>, VcsError>>()?;
            Ok(InstanceDelta::Tree(TreeRecordsDelta {
                records,
                removed: a[common..].to_vec(),
                appended: b[common..].to_vec(),
            }))
        }
        _ => Err(VcsError::ShapeMismatch {
            expected: before.shape(),
            found: after.shape(),
        }),
    }
}

/// Apply `delta` to `before`, an instance of the schema `schema`.
///
/// A tree delta applies when the sequence holds exactly the records the
/// delta covers and its removed records are the sequence's suffix, tree for
/// tree; each covered record's graph form takes its graph delta and is read
/// back as a tree, and the appended records follow.
///
/// # Errors
///
/// [`VcsError::ShapeMismatch`] when the delta and the instance have
/// different shapes, and [`VcsError::DeltaDoesNotApply`] when the delta's
/// checked reads do not hold of the instance (the shape's own refusal) or a
/// changed record is no longer a tree.
pub fn apply(
    schema: &Schema,
    delta: &InstanceDelta,
    before: &Instance,
) -> Result<Instance, VcsError> {
    let refused = |error: &dyn std::fmt::Display| VcsError::DeltaDoesNotApply {
        reason: error.to_string(),
    };
    match (delta, before) {
        (InstanceDelta::Table(delta), Instance::Table(table)) => {
            let mut out = table.clone();
            delta.apply(&mut out).map_err(|e| refused(&e))?;
            Ok(Instance::Table(out))
        }
        (InstanceDelta::Graph(delta), Instance::Graph(graph)) => {
            let mut out = graph.clone();
            delta.apply(&mut out).map_err(|e| refused(&e))?;
            Ok(Instance::Graph(out))
        }
        (InstanceDelta::Tree(delta), Instance::Tree(trees)) => {
            apply_tree(schema, delta, trees).map(Instance::Tree)
        }
        _ => Err(VcsError::ShapeMismatch {
            expected: delta.shape(),
            found: before.shape(),
        }),
    }
}

fn apply_tree(
    schema: &Schema,
    delta: &TreeRecordsDelta,
    trees: &[WInstance],
) -> Result<Vec<WInstance>, VcsError> {
    let common = delta.records.len();
    if trees.len() != common + delta.removed.len() {
        return Err(VcsError::DeltaDoesNotApply {
            reason: format!(
                "the delta covers {} records, the instance holds {}",
                common + delta.removed.len(),
                trees.len()
            ),
        });
    }
    let digest = panproto_inst::segment::winstance_digest;
    if let Some(index) = trees[common..]
        .iter()
        .zip(&delta.removed)
        .position(|(held, removed)| digest(held) != digest(removed))
    {
        return Err(VcsError::DeltaDoesNotApply {
            reason: format!(
                "record {} is not the record the delta removes",
                common + index
            ),
        });
    }
    let mut out = Vec::with_capacity(common + delta.appended.len());
    for (index, (tree, record)) in trees.iter().zip(&delta.records).enumerate() {
        let mut graph = graph_form(schema, tree)?;
        record
            .apply(&mut graph)
            .map_err(|error| VcsError::DeltaDoesNotApply {
                reason: format!("record {index}: {error}"),
            })?;
        let (tree, _) =
            graph
                .to_wtype_embedded(schema)
                .map_err(|error| VcsError::DeltaDoesNotApply {
                    reason: format!("record {index} is no longer a tree: {error}"),
                })?;
        out.push(tree);
    }
    out.extend(delta.appended.iter().cloned());
    Ok(out)
}

/// The key a data set is tracked under: its recorded key, or its
/// identifier when it records none.
#[must_use]
pub fn tracking_key(data_id: &ObjectId, dataset: &DataSetObject) -> String {
    dataset.key.clone().unwrap_or_else(|| data_id.to_string())
}

/// The data sets `commit` tracks, by key.
///
/// For each key it is the data set with that key the nearest commit on its
/// first-parent chain records, `commit` included, unless a nearer commit of
/// the chain stops tracking the key.
///
/// # Errors
///
/// A store error, [`VcsError::WrongObjectType`] when a recorded data set or
/// a first parent is another kind of object, and
/// [`VcsError::DuplicateDataKey`] when one commit of the chain records two
/// data sets under a key, or records a data set under a key it stops
/// tracking.
pub fn tracked(
    store: &dyn Store,
    commit: &CommitObject,
) -> Result<BTreeMap<String, (ObjectId, DataSetObject)>, VcsError> {
    let mut decided: BTreeMap<String, Option<(ObjectId, DataSetObject)>> = BTreeMap::new();
    let mut current = Some(commit.clone());
    while let Some(here) = current {
        let mut recorded: BTreeMap<String, Option<(ObjectId, DataSetObject)>> = here
            .untracked
            .iter()
            .map(|key| (key.clone(), None))
            .collect();
        for id in &here.data_ids {
            let dataset = match store.get(id)? {
                Object::DataSet(dataset) => dataset,
                other => {
                    return Err(VcsError::WrongObjectType {
                        expected: "dataset",
                        found: other.type_name(),
                    });
                }
            };
            let key = tracking_key(id, &dataset);
            if recorded.insert(key.clone(), Some((*id, dataset))).is_some() {
                return Err(VcsError::DuplicateDataKey { key });
            }
        }
        for (key, entry) in recorded {
            decided.entry(key).or_insert(entry);
        }
        current = match here.parents.first() {
            None => None,
            Some(parent) => match store.get(parent)? {
                Object::Commit(parent) => Some(parent),
                other => {
                    return Err(VcsError::WrongObjectType {
                        expected: "commit",
                        found: other.type_name(),
                    });
                }
            },
        };
    }
    Ok(decided
        .into_iter()
        .filter_map(|(key, entry)| entry.map(|entry| (key, entry)))
        .collect())
}

/// One key's change between two commits.
#[derive(Clone, Debug)]
pub enum InstanceChange {
    /// The second commit tracks a data set under a key the first does not.
    Added {
        /// The key.
        key: String,
        /// The added data set.
        data_id: ObjectId,
    },
    /// The first commit tracks a data set under a key the second does not.
    Removed {
        /// The key.
        key: String,
        /// The removed data set.
        data_id: ObjectId,
    },
    /// Both commits track a data set under the key, and they differ.
    Changed {
        /// The key.
        key: String,
        /// The first commit's data set.
        from: ObjectId,
        /// The second commit's data set.
        to: ObjectId,
        /// The delta from the first instance to the second.
        delta: InstanceDelta,
    },
}

impl InstanceChange {
    /// The key the change is under.
    #[must_use]
    pub fn key(&self) -> &str {
        match self {
            Self::Added { key, .. } | Self::Removed { key, .. } | Self::Changed { key, .. } => key,
        }
    }
}
