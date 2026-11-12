//! Instance version control, the reference for the Instance version control
//! correctness page.
//!
//! A repository is modelled as a set of named objects, each a record whose
//! reference fields name other objects, a set of refs, the `HEAD` state, the
//! reflog of the stash and the objects the staging index names. Which fields
//! of which kind of object are references is read off the object formats the
//! page states; reachability is a plain breadth-first search over them from
//! the roots, and garbage collection keeps exactly what the search reaches.
//! The definitions share no code with production.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// The shape of an instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Shape {
    /// Trees.
    Tree,
    /// Tables.
    Table,
    /// Graphs.
    Graph,
}

impl Shape {
    /// Every shape.
    pub const ALL: [Self; 3] = [Self::Tree, Self::Table, Self::Graph];
}

/// An object of the model, by kind, with the names its reference fields
/// hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Object {
    /// A schema object with no references: a file schema leaf or a flat
    /// schema.
    Leaf,
    /// A schema tree node, naming its leaves and subtrees.
    SchemaTree {
        /// The entries.
        entries: Vec<String>,
    },
    /// A migration between two schemas.
    Migration {
        /// The source schema.
        src: String,
        /// The target schema.
        tgt: String,
    },
    /// A commit.
    Commit {
        /// The commit's schema.
        schema: String,
        /// The migration from the first parent's schema, when recorded.
        migration: Option<String>,
        /// The parent commits.
        parents: Vec<String>,
        /// The data sets.
        data: Vec<String>,
        /// The complements.
        complements: Vec<String>,
        /// The edit logs.
        edit_logs: Vec<String>,
        /// The protocol, theories and CST complements it records.
        metadata: Vec<String>,
    },
    /// An annotated tag.
    Tag {
        /// The tagged object.
        target: String,
    },
    /// A data set of a shape.
    DataSet {
        /// The instance's shape.
        shape: Shape,
        /// The schema it conforms to.
        schema: String,
    },
    /// A complement of a shape.
    Complement {
        /// The complement's shape.
        shape: Shape,
        /// The migration that produced it.
        migration: String,
        /// The data set it was computed from.
        data: String,
    },
    /// An edit log of a shape.
    EditLog {
        /// The edits' shape.
        shape: Shape,
        /// The schema the edits apply to.
        schema: String,
        /// The data set they were applied to.
        data: String,
        /// The complement after the last edit.
        final_complement: String,
    },
}

impl Object {
    /// The names the object's reference fields hold.
    #[must_use]
    pub fn links(&self) -> Vec<&str> {
        match self {
            Self::Leaf => Vec::new(),
            Self::SchemaTree { entries } => entries.iter().map(String::as_str).collect(),
            Self::Migration { src, tgt } => vec![src.as_str(), tgt.as_str()],
            Self::Commit {
                schema,
                migration,
                parents,
                data,
                complements,
                edit_logs,
                metadata,
            } => std::iter::once(schema.as_str())
                .chain(migration.as_deref())
                .chain(parents.iter().map(String::as_str))
                .chain(data.iter().map(String::as_str))
                .chain(complements.iter().map(String::as_str))
                .chain(edit_logs.iter().map(String::as_str))
                .chain(metadata.iter().map(String::as_str))
                .collect(),
            Self::Tag { target } => vec![target.as_str()],
            Self::DataSet { schema, .. } => vec![schema.as_str()],
            Self::Complement {
                migration, data, ..
            } => vec![migration.as_str(), data.as_str()],
            Self::EditLog {
                schema,
                data,
                final_complement,
                ..
            } => vec![schema.as_str(), data.as_str(), final_complement.as_str()],
        }
    }
}

/// Where `HEAD` points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Head {
    /// On a branch, named without the `refs/heads/` prefix; the branch may
    /// not exist yet.
    Branch(String),
    /// Detached at an object.
    Detached(String),
}

/// A repository of the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repository {
    /// The stored objects, by name.
    pub objects: BTreeMap<String, Object>,
    /// The refs, by full name, each naming an object.
    pub refs: BTreeMap<String, String>,
    /// `HEAD`.
    pub head: Head,
    /// The objects the stash's reflog names, one per entry.
    pub stash_log: Vec<String>,
    /// The objects the staging index names.
    pub staged: Vec<String>,
}

impl Repository {
    /// The roots: what `HEAD` resolves to, the target of every ref under
    /// `refs/`, every stash reflog entry and every staged object.
    #[must_use]
    pub fn roots(&self) -> BTreeSet<&str> {
        let mut roots = BTreeSet::new();
        match &self.head {
            Head::Branch(name) => {
                if let Some(target) = self.refs.get(&format!("refs/heads/{name}")) {
                    roots.insert(target.as_str());
                }
            }
            Head::Detached(target) => {
                roots.insert(target.as_str());
            }
        }
        for (name, target) in &self.refs {
            if name.starts_with("refs/") {
                roots.insert(target.as_str());
            }
        }
        roots.extend(self.stash_log.iter().map(String::as_str));
        roots.extend(self.staged.iter().map(String::as_str));
        roots
    }

    /// Every stored object a path of references leads to from a root,
    /// found by breadth-first search. A name that is not stored is not
    /// followed and not returned.
    #[must_use]
    pub fn reachable(&self) -> BTreeSet<String> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut queue: VecDeque<&str> = self.roots().into_iter().collect();
        while let Some(name) = queue.pop_front() {
            let Some(object) = self.objects.get(name) else {
                continue;
            };
            if seen.insert(name.to_owned()) {
                queue.extend(object.links());
            }
        }
        seen
    }

    /// What garbage collection deletes: every stored object that is not
    /// reachable.
    #[must_use]
    pub fn garbage(&self) -> BTreeSet<String> {
        let reachable = self.reachable();
        self.objects
            .keys()
            .filter(|name| !reachable.contains(*name))
            .cloned()
            .collect()
    }
}

/// What decoding a payload gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PayloadOutcome {
    /// The payload decodes to its value.
    Decoded,
    /// The encoding's version is not 1.
    Unsupported,
    /// The bytes do not decode whole as the stated value, or the value
    /// contradicts the object's count.
    Undecodable,
    /// The bytes decode, but are not the encoding of their value.
    NonCanonical,
}

/// The facts about a payload that decide what decoding it gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PayloadFacts {
    /// The shape the object records.
    pub shape: Shape,
    /// The version the object records.
    pub version: u32,
    /// Whether the bytes are exactly one value of the shape's payload type.
    pub decodes_whole: bool,
    /// Whether the bytes are the encoding of that value.
    pub canonical: bool,
    /// Whether the object's record or edit count is the value's.
    pub count_agrees: bool,
}

/// What decoding a payload with these facts gives.
///
/// The version is checked first, then that the bytes decode whole, then,
/// for a table or graph
/// payload, that they are canonical, and last the count. A tree payload's
/// canonicity is not checked.
#[must_use]
pub const fn payload_outcome(facts: &PayloadFacts) -> PayloadOutcome {
    if facts.version != 1 {
        PayloadOutcome::Unsupported
    } else if !facts.decodes_whole {
        PayloadOutcome::Undecodable
    } else if !matches!(facts.shape, Shape::Tree) && !facts.canonical {
        PayloadOutcome::NonCanonical
    } else if !facts.count_agrees {
        PayloadOutcome::Undecodable
    } else {
        PayloadOutcome::Decoded
    }
}

/// Whether an object a data migration reads is stored, and of its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stored {
    /// No object is stored under the identifier.
    Missing,
    /// The object stored is of another kind.
    WrongKind,
    /// The object is stored and of its kind.
    Present,
}

/// An object a data migration reads, in the order it reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MigrationRead {
    /// The data set migrated.
    DataSet,
    /// The complement put back against (backward only).
    Complement,
    /// The migration.
    Migration,
    /// The migration's source schema.
    Source,
    /// The migration's target schema.
    Target,
    /// The data set's schema.
    DataSchema,
}

/// What a forward or backward data migration gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MigrationOutcome {
    /// An object read is not stored.
    Missing(MigrationRead),
    /// An object read is of another kind.
    WrongKind(MigrationRead),
    /// The complement names no migration.
    UnknownMigration,
    /// The data set does not conform to the schema the migration starts
    /// from (forward) or reaches (backward).
    SchemaMismatch,
    /// The migration does not compile between its schemas.
    DoesNotCompile,
    /// The complement holds another shape than the data set.
    ShapeMismatch,
    /// A tree complement holds another number of complements than the data
    /// set holds records.
    RecordCountMismatch,
    /// The lens refuses: get refuses the instance, or put refuses the
    /// complement's identity or the instance against it.
    LensRefuses,
    /// The data migrates.
    Migrates,
}

/// The facts only a backward migration reads: those of the complement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BackwardFacts {
    /// The complement.
    pub complement: Stored,
    /// Whether the complement names a migration.
    pub names_migration: bool,
    /// Whether the complement holds the data set's shape.
    pub shape_agrees: bool,
    /// Whether a tree complement holds one complement per record.
    pub count_agrees: bool,
}

/// The facts that decide what a data migration gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MigrationFacts {
    /// The complement's facts for a backward migration (a put against a
    /// complement), `None` for a forward one.
    pub backward: Option<BackwardFacts>,
    /// The data set.
    pub data: Stored,
    /// The migration.
    pub migration: Stored,
    /// The migration's source schema.
    pub source: Stored,
    /// The migration's target schema.
    pub target: Stored,
    /// The data set's schema.
    pub data_schema: Stored,
    /// Whether the data set's schema is the migration's source (forward)
    /// or target (backward).
    pub conforms: bool,
    /// Whether the migration compiles between its schemas.
    pub compiles: bool,
    /// Whether the lens accepts the instance (and, backward, the complement).
    pub lens_accepts: bool,
}

/// The outcome of a stored-object fact, or `None` when the object is
/// present.
const fn read(stored: Stored, object: MigrationRead) -> Option<MigrationOutcome> {
    match stored {
        Stored::Missing => Some(MigrationOutcome::Missing(object)),
        Stored::WrongKind => Some(MigrationOutcome::WrongKind(object)),
        Stored::Present => None,
    }
}

/// What a data migration with these facts gives.
///
/// The data set is read first; backward, the complement next, and a
/// complement naming no migration stops there. Then the migration, its
/// source and target schemas and the data set's schema are read in that
/// order; then the data set must conform; then the migration must compile;
/// backward, the complement must hold the data set's shape and, for trees,
/// one complement per record; and last the lens must accept.
#[must_use]
pub const fn migration_outcome(facts: &MigrationFacts) -> MigrationOutcome {
    if let Some(outcome) = read(facts.data, MigrationRead::DataSet) {
        return outcome;
    }
    if let Some(backward) = facts.backward {
        if let Some(outcome) = read(backward.complement, MigrationRead::Complement) {
            return outcome;
        }
        if !backward.names_migration {
            return MigrationOutcome::UnknownMigration;
        }
    }
    let reads = [
        (facts.migration, MigrationRead::Migration),
        (facts.source, MigrationRead::Source),
        (facts.target, MigrationRead::Target),
        (facts.data_schema, MigrationRead::DataSchema),
    ];
    let mut at = 0;
    while at < reads.len() {
        if let Some(outcome) = read(reads[at].0, reads[at].1) {
            return outcome;
        }
        at += 1;
    }
    if !facts.conforms {
        return MigrationOutcome::SchemaMismatch;
    }
    if !facts.compiles {
        return MigrationOutcome::DoesNotCompile;
    }
    if let Some(backward) = facts.backward {
        if !backward.shape_agrees {
            return MigrationOutcome::ShapeMismatch;
        }
        if !backward.count_agrees {
            return MigrationOutcome::RecordCountMismatch;
        }
    }
    if facts.lens_accepts {
        MigrationOutcome::Migrates
    } else {
        MigrationOutcome::LensRefuses
    }
}

/// The objects a forward migration of `data` along `migration` adds to a
/// repository.
///
/// They are the migrated data set, of `shape` and naming the schema tree
/// `target_tree`, and the complement, naming `migration` and `data`.
/// Backward migration adds a data set of the shape naming the schema tree of
/// the migration's source.
#[must_use]
pub fn forward_objects(
    shape: Shape,
    data: &str,
    migration: &str,
    target_tree: &str,
) -> (Object, Object) {
    (
        Object::DataSet {
            shape,
            schema: target_tree.to_owned(),
        },
        Object::Complement {
            shape,
            migration: migration.to_owned(),
            data: data.to_owned(),
        },
    )
}

/// The encoding an object holds given the encoding field it records.
///
/// It is the recorded shape and version, or version 1 of the tree encoding
/// when the object records none.
#[must_use]
pub const fn recorded_encoding(field: Option<(Shape, u32)>) -> (Shape, u32) {
    match field {
        Some(encoding) => encoding,
        None => (Shape::Tree, 1),
    }
}

/// Whether an object holding `encoding` writes the encoding field.
///
/// Every encoding but version 1 of the tree encoding is written.
#[must_use]
pub const fn writes_encoding_field(encoding: (Shape, u32)) -> bool {
    !(matches!(encoding.0, Shape::Tree) && encoding.1 == 1)
}

/// An instance as a data set holds it: a sequence of trees, a table or a
/// graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instance {
    /// Trees, one per record.
    Trees(Vec<crate::model::Tree>),
    /// A table instance.
    Table(crate::model::Table),
    /// A graph instance.
    Graph(crate::model::Graph),
}

/// The delta between two instances of one shape.
#[derive(Clone, Debug, PartialEq)]
pub enum Delta {
    /// Between two sequences of trees: the graph delta between the graph
    /// forms of each record both hold, and the records only one holds.
    Trees {
        /// One graph delta per record both sequences hold.
        records: Vec<crate::edit::GraphDelta>,
        /// The first sequence's records past the second's length.
        removed: Vec<crate::model::Tree>,
        /// The second sequence's records past the first's length.
        appended: Vec<crate::model::Tree>,
    },
    /// Between two tables.
    Table(crate::edit::TableDelta),
    /// Between two graphs.
    Graph(crate::edit::GraphDelta),
}

/// Why a diff or an application is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The two sides have different shapes.
    ShapeMismatch,
    /// The two data sets of the key conform to different schemas.
    SchemaMismatch(String),
    /// A tree has no graph form, or the delta does not apply.
    DoesNotApply,
}

/// The graph form of a tree without fans: arc `i` is edge `i`.
fn graph_form(tree: &crate::model::Tree) -> Result<crate::model::Graph, Refusal> {
    crate::pages::representation::encode::tree_to_graph(tree, None)
        .map(|(graph, _)| graph)
        .map_err(|_| Refusal::DoesNotApply)
}

/// The tree a graph form is: rooted at its one node no edge enters.
fn tree_form(graph: &crate::model::Graph) -> Result<crate::model::Tree, Refusal> {
    let entered: BTreeSet<u32> = graph.edges.values().map(|edge| edge.tgt).collect();
    let roots: Vec<u32> = graph
        .nodes
        .keys()
        .copied()
        .filter(|id| !entered.contains(id))
        .collect();
    let [root] = roots.as_slice() else {
        return Err(Refusal::DoesNotApply);
    };
    crate::pages::representation::encode::graph_to_tree(graph, *root, &|_| {
        crate::pages::representation::encode::ParentNameRule::Any
    })
    .map_err(|_| Refusal::DoesNotApply)
}

/// A tree with its arcs listed by child, so two trees with the same
/// nodes, root, arcs and fans compare equal whatever order their arcs are
/// listed in.
#[must_use]
pub fn normalized(tree: &crate::model::Tree) -> crate::model::Tree {
    let mut out = tree.clone();
    out.arcs.sort_by_key(|arc| (arc.1, arc.0));
    out
}

/// The delta from `before` to `after`: the table or graph differ of the
/// edit algebra, or for trees the graph differ of each common record's graph
/// form with the records only one side holds.
///
/// # Errors
///
/// [`Refusal::ShapeMismatch`] for two shapes, and [`Refusal::DoesNotApply`]
/// for a tree without a graph form.
pub fn diff_instances(before: &Instance, after: &Instance) -> Result<Delta, Refusal> {
    match (before, after) {
        (Instance::Table(a), Instance::Table(b)) => Ok(Delta::Table(crate::edit::diff_table(a, b))),
        (Instance::Graph(a), Instance::Graph(b)) => Ok(Delta::Graph(crate::edit::diff(a, b))),
        (Instance::Trees(a), Instance::Trees(b)) => {
            let common = a.len().min(b.len());
            let mut records = Vec::new();
            for index in 0..common {
                records.push(crate::edit::diff(
                    &graph_form(&a[index])?,
                    &graph_form(&b[index])?,
                ));
            }
            Ok(Delta::Trees {
                records,
                removed: a[common..].to_vec(),
                appended: b[common..].to_vec(),
            })
        }
        _ => Err(Refusal::ShapeMismatch),
    }
}

/// Apply `delta` to `before`.
///
/// A tree delta applies when the sequence holds exactly the records it
/// covers and its removed records are, after normalizing, the sequence's
/// suffix.
///
/// # Errors
///
/// [`Refusal::ShapeMismatch`] for two shapes, and [`Refusal::DoesNotApply`]
/// when the edit algebra refuses or a record is no longer a tree.
pub fn apply_delta(delta: &Delta, before: &Instance) -> Result<Instance, Refusal> {
    match (delta, before) {
        (Delta::Table(d), Instance::Table(t)) => crate::edit::apply_table(d, t)
            .map(Instance::Table)
            .map_err(|_| Refusal::DoesNotApply),
        (Delta::Graph(d), Instance::Graph(g)) => crate::edit::apply(d, g)
            .map(Instance::Graph)
            .map_err(|_| Refusal::DoesNotApply),
        (
            Delta::Trees {
                records,
                removed,
                appended,
            },
            Instance::Trees(trees),
        ) => {
            let common = records.len();
            if trees.len() != common + removed.len()
                || trees[common..]
                    .iter()
                    .zip(removed)
                    .any(|(held, gone)| normalized(held) != normalized(gone))
            {
                return Err(Refusal::DoesNotApply);
            }
            let mut out = Vec::new();
            for (tree, record) in trees.iter().zip(records) {
                let graph = crate::edit::apply(record, &graph_form(tree)?)
                    .map_err(|_| Refusal::DoesNotApply)?;
                out.push(tree_form(&graph)?);
            }
            out.extend(appended.iter().cloned());
            Ok(Instance::Trees(out))
        }
        _ => Err(Refusal::ShapeMismatch),
    }
}

/// A data set as a commit records it: its name, the name of its schema and
/// its instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The data set's name.
    pub data: String,
    /// Its schema's name.
    pub schema: String,
    /// Its instance.
    pub instance: Instance,
}

/// The entries a commit tracks by key.
///
/// The chain lists what each commit of the first-parent chain records under
/// each key, the commit itself first: an entry, or `None` when the commit
/// stops tracking the key. For each key it is the entry of the first commit
/// that records something under the key, and the key is not tracked when
/// that commit stops tracking it.
#[must_use]
pub fn tracked(chain: &[BTreeMap<String, Option<Entry>>]) -> BTreeMap<String, Entry> {
    let mut decided: BTreeMap<String, Option<Entry>> = BTreeMap::new();
    for recorded in chain {
        for (key, entry) in recorded {
            decided.entry(key.clone()).or_insert_with(|| entry.clone());
        }
    }
    decided
        .into_iter()
        .filter_map(|(key, entry)| entry.map(|entry| (key, entry)))
        .collect()
}

/// One key's change between two tracked maps.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// Tracked by the second only.
    Added(String),
    /// Tracked by the first only.
    Removed(String),
    /// Tracked by both under different data sets, with the delta.
    Changed(String, Delta),
}

/// The changes between two tracked maps, in key order: a key whose data
/// sets differ is changed by the delta of its instances.
///
/// # Errors
///
/// For the first key in order whose data sets differ:
/// [`Refusal::SchemaMismatch`] when their schemas differ, and otherwise the
/// refusal of [`diff_instances`].
pub fn diff_tracked(
    before: &BTreeMap<String, Entry>,
    after: &BTreeMap<String, Entry>,
) -> Result<Vec<Change>, Refusal> {
    let keys: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    let mut out = Vec::new();
    for key in keys {
        match (before.get(key), after.get(key)) {
            (Some(_), None) => out.push(Change::Removed(key.clone())),
            (None, Some(_)) => out.push(Change::Added(key.clone())),
            (Some(a), Some(b)) if a.data != b.data => {
                if a.schema != b.schema {
                    return Err(Refusal::SchemaMismatch(key.clone()));
                }
                out.push(Change::Changed(
                    key.clone(),
                    diff_instances(&a.instance, &b.instance)?,
                ));
            }
            _ => {}
        }
    }
    Ok(out)
}

/// A side of a merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    /// Ours.
    Ours,
    /// Theirs.
    Theirs,
}

/// A conflict over a whole key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrackingConflict {
    /// The base does not track the key and the sides track different data
    /// sets under it.
    AddedOnBothSides,
    /// The side stops tracking the key and the other changes it.
    RemovedAndChanged(Side),
    /// The three shapes are not all one.
    ShapeDiffers,
    /// The three schemas are not all one.
    SchemaDiffers,
}

/// A conflict over one record of a sequence of trees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecordConflict {
    /// Both sides remove the record.
    RemovedByBoth,
    /// The side removes the record and the other changes it.
    RemovedAndChanged(Side),
    /// Both sides append records.
    AppendedByBoth,
    /// The merged graph form of the record is not a tree.
    NotATree,
}

/// One of the three versions of a key a merge combines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Version {
    /// The merge base's (a replayed commit's first parent's).
    Base,
    /// Ours (the tip's, for a replay).
    Ours,
    /// Theirs (the replayed commit's).
    Theirs,
}

/// Why a version of a key does not migrate to the merged schema.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MigrationRefusal {
    /// The path does not exist ([`migration_path`]).
    Path(PathRefusal),
    /// The named migration does not compile.
    DoesNotCompile(String),
    /// Get along a migration of the path refuses the instance, at the given
    /// record of a sequence of trees.
    GetRefused(Option<usize>),
}

/// A conflict of an instance merge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conflict {
    /// Over a key.
    Tracking(String, TrackingConflict),
    /// A version of the key does not migrate to the merged schema.
    Migration(String, Version, MigrationRefusal),
    /// Between two graph deltas: of a graph, or of a tree record's graph
    /// form.
    GraphEdit(String, Option<usize>, crate::edit::EditConflict),
    /// Between two table deltas.
    TableEdit(String, crate::edit::TableConflict),
    /// Over a tree record.
    Record(String, usize, RecordConflict),
}

/// What a merge gives a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Merged {
    /// The data set, by name, that a side tracks.
    Kept(String),
    /// A merged instance of the named schema.
    New(String, Instance),
}

/// The three-way merge of the entries two sides track against the entries
/// their base tracks: what it gives each key it tracks, and the conflicts.
///
/// Key by key in ascending order: a key the two sides track under one data
/// set, or neither tracks, keeps that; a key one side tracks as the base
/// does takes the other side's entry or absence. Otherwise a key the base
/// does not track is added on both sides, a key one side does not track is
/// removed by it and changed by the other, three shapes or three schemas
/// that are not one are a conflict, and three instances merge by
/// [`merge_instances`].
///
/// # Errors
///
/// [`Refusal::DoesNotApply`] when a tree has no graph form or the merge of
/// independent deltas does not apply.
pub fn merge_tracked(
    base: &BTreeMap<String, Entry>,
    ours: &BTreeMap<String, Entry>,
    theirs: &BTreeMap<String, Entry>,
) -> Result<(BTreeMap<String, Merged>, Vec<Conflict>), Refusal> {
    merge_tracked_with(base, ours, theirs, None)
}

/// A merge's migration of each version of a key to the merged schema: the
/// merged schema's name, and the instance a version's entry migrates to, or
/// why it does not.
pub type ToMerged<'a> = (
    &'a str,
    &'a mut dyn FnMut(Version, &Entry) -> Result<Instance, MigrationRefusal>,
);

/// The three-way merge of [`merge_tracked`], migrating versions to the
/// merged schema.
///
/// Three versions of a key under schemas that are not all one are not
/// [`TrackingConflict::SchemaDiffers`]: they combine by
/// [`migrate_and_combine`] into an instance of the merged schema.
///
/// # Errors
///
/// As [`merge_tracked`].
pub fn merge_tracked_to(
    base: &BTreeMap<String, Entry>,
    ours: &BTreeMap<String, Entry>,
    theirs: &BTreeMap<String, Entry>,
    to: ToMerged<'_>,
) -> Result<(BTreeMap<String, Merged>, Vec<Conflict>), Refusal> {
    merge_tracked_with(base, ours, theirs, Some(to))
}

/// Combine three versions of a key recorded against different schemas.
///
/// Each migrates to the merged schema by `migrate`, base first, then ours,
/// then theirs; every version that does not is one [`Conflict::Migration`],
/// and otherwise the migrated instances merge by [`merge_instances`] (by
/// [`rebase_instances`] when `rebase`).
///
/// # Errors
///
/// As [`merge_instances`].
pub fn migrate_and_combine(
    rebase: bool,
    key: &str,
    versions: [&Entry; 3],
    migrate: &mut dyn FnMut(Version, &Entry) -> Result<Instance, MigrationRefusal>,
) -> Result<Result<Instance, Vec<Conflict>>, Refusal> {
    let mut migrated = Vec::new();
    let mut refused = Vec::new();
    for (version, entry) in [Version::Base, Version::Ours, Version::Theirs]
        .into_iter()
        .zip(versions)
    {
        match migrate(version, entry) {
            Ok(instance) => migrated.push(instance),
            Err(refusal) => refused.push(Conflict::Migration(key.to_owned(), version, refusal)),
        }
    }
    if !refused.is_empty() {
        return Ok(Err(refused));
    }
    combine(rebase, key, &migrated[0], &migrated[1], &migrated[2])
}

fn merge_tracked_with(
    base: &BTreeMap<String, Entry>,
    ours: &BTreeMap<String, Entry>,
    theirs: &BTreeMap<String, Entry>,
    mut to: Option<ToMerged<'_>>,
) -> Result<(BTreeMap<String, Merged>, Vec<Conflict>), Refusal> {
    let keys: BTreeSet<&String> = base
        .keys()
        .chain(ours.keys())
        .chain(theirs.keys())
        .collect();
    let mut merged = BTreeMap::new();
    let mut conflicts = Vec::new();
    for key in keys {
        let (b, o, t) = (base.get(key), ours.get(key), theirs.get(key));
        let name = |entry: Option<&Entry>| entry.map(|e| e.data.clone());
        let tracking = |conflict| Conflict::Tracking(key.clone(), conflict);
        let chosen = if name(o) == name(t) || name(t) == name(b) {
            Some(o)
        } else if name(o) == name(b) {
            Some(t)
        } else {
            None
        };
        if let Some(chosen) = chosen {
            if let Some(entry) = chosen {
                merged.insert(key.clone(), Merged::Kept(entry.data.clone()));
            }
            continue;
        }
        match (b, o, t) {
            (None, _, _) => conflicts.push(tracking(TrackingConflict::AddedOnBothSides)),
            (_, None, _) => {
                conflicts.push(tracking(TrackingConflict::RemovedAndChanged(Side::Ours)));
            }
            (_, _, None) => {
                conflicts.push(tracking(TrackingConflict::RemovedAndChanged(Side::Theirs)));
            }
            (Some(b), Some(o), Some(t)) => {
                let shape = |entry: &Entry| match entry.instance {
                    Instance::Trees(_) => Shape::Tree,
                    Instance::Table(_) => Shape::Table,
                    Instance::Graph(_) => Shape::Graph,
                };
                if shape(b) != shape(o) || shape(b) != shape(t) {
                    conflicts.push(tracking(TrackingConflict::ShapeDiffers));
                } else if b.schema != o.schema || b.schema != t.schema {
                    match to.as_mut() {
                        None => conflicts.push(tracking(TrackingConflict::SchemaDiffers)),
                        Some((merged_schema, migrate)) => {
                            match migrate_and_combine(false, key, [b, o, t], &mut **migrate)? {
                                Ok(instance) => {
                                    merged.insert(
                                        key.clone(),
                                        Merged::New((*merged_schema).to_owned(), instance),
                                    );
                                }
                                Err(found) => conflicts.extend(found),
                            }
                        }
                    }
                } else {
                    match merge_instances(key, &b.instance, &o.instance, &t.instance)? {
                        Ok(instance) => {
                            merged.insert(key.clone(), Merged::New(b.schema.clone(), instance));
                        }
                        Err(found) => conflicts.extend(found),
                    }
                }
            }
        }
    }
    Ok((merged, conflicts))
}

/// The three-way merge of three instances of one shape under `key`: the
/// instance the merge of the two deltas from the base gives the base, or
/// the conflicts.
///
/// Graphs and tables merge by [`crate::edit::merge`] and
/// [`crate::edit::merge_table`], with every conflict of the deltas'
/// independence when they are not independent. Sequences of trees merge
/// record by record: a record both keep merges through the graph forms of
/// its three versions and must read back as a tree; a record one side
/// removes is removed when the other leaves its graph form unchanged and
/// is otherwise removed by one and changed by the other; a record both
/// remove conflicts; the records one side appends follow the merged
/// records, and records both sides append conflict at the base's length.
///
/// # Errors
///
/// [`Refusal::ShapeMismatch`] for instances of different shapes, and
/// [`Refusal::DoesNotApply`] when a tree has no graph form or the merge of
/// independent deltas does not apply.
pub fn merge_instances(
    key: &str,
    base: &Instance,
    ours: &Instance,
    theirs: &Instance,
) -> Result<Result<Instance, Vec<Conflict>>, Refusal> {
    combine(false, key, base, ours, theirs)
}

/// The replay onto `tip` of the change from `parent` to `replayed`.
///
/// It is [`merge_instances`] with `parent` as base, `tip` as ours and
/// `replayed` as theirs, except that two graph or table deltas combine by
/// rebasing: the replayed delta, rebased over the tip's by
/// [`crate::edit::rebase`] or [`crate::edit::rebase_table`], applies to the
/// tip's instance, and a refused rebase is its one conflict.
///
/// # Errors
///
/// As [`merge_instances`].
pub fn rebase_instances(
    key: &str,
    parent: &Instance,
    tip: &Instance,
    replayed: &Instance,
) -> Result<Result<Instance, Vec<Conflict>>, Refusal> {
    combine(true, key, parent, tip, replayed)
}

fn combine(
    rebase: bool,
    key: &str,
    base: &Instance,
    ours: &Instance,
    theirs: &Instance,
) -> Result<Result<Instance, Vec<Conflict>>, Refusal> {
    match (base, ours, theirs) {
        (Instance::Graph(b), Instance::Graph(o), Instance::Graph(t)) => {
            Ok(merge_graphs(rebase, key, None, b, o, t)?.map(Instance::Graph))
        }
        (Instance::Table(table), Instance::Table(ours), Instance::Table(theirs)) => {
            merge_tables(rebase, key, table, ours, theirs)
        }
        (Instance::Trees(b), Instance::Trees(o), Instance::Trees(t)) => {
            merge_tree_records(rebase, key, b, o, t)
        }
        _ => Err(Refusal::ShapeMismatch),
    }
}

fn merge_tables(
    rebase: bool,
    key: &str,
    base: &crate::model::Table,
    ours: &crate::model::Table,
    theirs: &crate::model::Table,
) -> Result<Result<Instance, Vec<Conflict>>, Refusal> {
    let from_ours = crate::edit::diff_table(base, ours);
    let from_theirs = crate::edit::diff_table(base, theirs);
    if rebase {
        return Ok(
            match crate::edit::rebase_table(&from_theirs, std::slice::from_ref(&from_ours)) {
                Ok(delta) => Ok(Instance::Table(
                    crate::edit::apply_table(&delta, ours).map_err(|_| Refusal::DoesNotApply)?,
                )),
                Err(refused) => Err(vec![Conflict::TableEdit(key.to_owned(), *refused.conflict)]),
            },
        );
    }
    let found = crate::edit::table_independence(&from_ours, &from_theirs);
    if found.is_empty() {
        let delta = crate::edit::merge_table(&from_ours, &from_theirs)
            .map_err(|_| Refusal::DoesNotApply)?;
        let table = crate::edit::apply_table(&delta, base).map_err(|_| Refusal::DoesNotApply)?;
        Ok(Ok(Instance::Table(table)))
    } else {
        Ok(Err(found
            .into_iter()
            .map(|conflict| Conflict::TableEdit(key.to_owned(), conflict))
            .collect()))
    }
}

fn merge_graphs(
    rebase: bool,
    key: &str,
    record: Option<usize>,
    base: &crate::model::Graph,
    ours: &crate::model::Graph,
    theirs: &crate::model::Graph,
) -> Result<Result<crate::model::Graph, Vec<Conflict>>, Refusal> {
    let (x, y) = (
        crate::edit::diff(base, ours),
        crate::edit::diff(base, theirs),
    );
    if rebase {
        return Ok(match crate::edit::rebase(&y, std::slice::from_ref(&x)) {
            Ok(delta) => Ok(crate::edit::apply(&delta, ours).map_err(|_| Refusal::DoesNotApply)?),
            Err(refused) => Err(vec![Conflict::GraphEdit(
                key.to_owned(),
                record,
                *refused.conflict,
            )]),
        });
    }
    let found = crate::edit::independence(&x, &y);
    if found.is_empty() {
        let delta = crate::edit::merge(&x, &y).map_err(|_| Refusal::DoesNotApply)?;
        let graph = crate::edit::apply(&delta, base).map_err(|_| Refusal::DoesNotApply)?;
        Ok(Ok(graph))
    } else {
        Ok(Err(found
            .into_iter()
            .map(|conflict| Conflict::GraphEdit(key.to_owned(), record, conflict))
            .collect()))
    }
}

fn merge_tree_records(
    rebase: bool,
    key: &str,
    base: &[crate::model::Tree],
    ours: &[crate::model::Tree],
    theirs: &[crate::model::Tree],
) -> Result<Result<Instance, Vec<Conflict>>, Refusal> {
    let mut conflicts = Vec::new();
    let mut merged = Vec::new();
    let record = |index: usize, conflict| Conflict::Record(key.to_owned(), index, conflict);
    for (index, tree) in base.iter().enumerate() {
        let b = graph_form(tree)?;
        let kept_by_ours = index < ours.len();
        let kept_by_theirs = index < theirs.len();
        if kept_by_ours && kept_by_theirs {
            let (o, t) = (graph_form(&ours[index])?, graph_form(&theirs[index])?);
            match merge_graphs(rebase, key, Some(index), &b, &o, &t)? {
                Ok(graph) => match tree_form(&graph) {
                    Ok(tree) => merged.push(tree),
                    Err(_) => conflicts.push(record(index, RecordConflict::NotATree)),
                },
                Err(found) => conflicts.extend(found),
            }
        } else if !kept_by_ours && !kept_by_theirs {
            conflicts.push(record(index, RecordConflict::RemovedByBoth));
        } else {
            let (remover, keeper) = if kept_by_ours {
                (Side::Theirs, &ours[index])
            } else {
                (Side::Ours, &theirs[index])
            };
            if crate::edit::diff(&b, &graph_form(keeper)?) != crate::edit::GraphDelta::identity() {
                conflicts.push(record(index, RecordConflict::RemovedAndChanged(remover)));
            }
        }
    }
    let appended_ours = ours.len() > base.len();
    let appended_theirs = theirs.len() > base.len();
    if appended_ours && appended_theirs {
        conflicts.push(record(base.len(), RecordConflict::AppendedByBoth));
    }
    if !conflicts.is_empty() {
        return Ok(Err(conflicts));
    }
    if appended_ours {
        merged.extend(ours[base.len()..].iter().cloned());
    }
    if appended_theirs {
        merged.extend(theirs[base.len()..].iter().cloned());
    }
    Ok(Ok(Instance::Trees(merged)))
}

/// A commit of a first-parent chain as a migration path reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainCommit {
    /// The commit's name.
    pub name: String,
    /// The identity of its schema.
    pub schema: String,
    /// The migration it records, by name.
    pub migration: Option<String>,
}

/// Why first-parent history has no migration path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PathRefusal {
    /// No commit of the chain has the schema.
    NoPath,
    /// The named commit changes its parent's schema and records no
    /// migration.
    MissingLink(String),
    /// The named commit records a migration that does not run from its
    /// parent's schema to its own.
    LinkMismatch(String),
}

/// The migrations first-parent history records from `schema` to the schema
/// of the chain's first commit, oldest first.
///
/// `chain` lists the commit, then its first parent, and so on to a root;
/// `migrations` gives each stored migration's source and target schema
/// identities by name. The path starts at the first commit of `chain` (the
/// nearest) whose schema is `schema`. Walking from there towards the
/// chain's first commit, a commit with its parent's schema adds nothing,
/// and any other commit adds its migration, which must run from its
/// parent's schema to its own.
///
/// # Errors
///
/// [`PathRefusal::NoPath`] when no commit of the chain has `schema`, and
/// otherwise the refusal of the oldest link that fails:
/// [`PathRefusal::MissingLink`] when it records no migration,
/// [`PathRefusal::LinkMismatch`] when its migration (or a name `migrations`
/// does not hold) does not run from its parent's schema to its own.
pub fn migration_path(
    chain: &[ChainCommit],
    migrations: &BTreeMap<String, (String, String)>,
    schema: &str,
) -> Result<Vec<String>, PathRefusal> {
    let Some(start) = chain.iter().position(|commit| commit.schema == schema) else {
        return Err(PathRefusal::NoPath);
    };
    let mut path = Vec::new();
    let mut index = start;
    while index > 0 {
        let parent = &chain[index];
        let commit = &chain[index - 1];
        index -= 1;
        if commit.schema == parent.schema {
            continue;
        }
        let Some(migration) = &commit.migration else {
            return Err(PathRefusal::MissingLink(commit.name.clone()));
        };
        if migrations.get(migration) != Some(&(parent.schema.clone(), commit.schema.clone())) {
            return Err(PathRefusal::LinkMismatch(commit.name.clone()));
        }
        path.push(migration.clone());
    }
    Ok(path)
}

/// One side of a merge as its migration path reads it.
///
/// It is the first-parent chain of the side's commit, and the side's
/// migration to the merged schema by name (none when the side's schema is
/// the merged schema).
#[derive(Clone, Copy, Debug)]
pub struct PathSide<'a> {
    /// The side commit's first-parent chain, the commit first.
    pub chain: &'a [ChainCommit],
    /// The migration from the side's schema to the merged schema.
    pub migration: Option<&'a str>,
}

/// The migrations from `schema`, the schema of a data set `version` tracks,
/// to the merged schema `merged`.
///
/// The merged schema itself has the empty path. Otherwise the path is the
/// [`migration_path`] along a side's chain followed by the side's
/// migration: for ours along our chain, for theirs along theirs, and for the
/// base along ours when our chain holds a commit of `schema` and else along
/// theirs.
///
/// # Errors
///
/// The refusal of [`migration_path`]; for the base, when neither chain
/// holds a commit of `schema`, [`PathRefusal::NoPath`].
pub fn merge_path(
    version: Version,
    schema: &str,
    [ours, theirs]: [PathSide<'_>; 2],
    migrations: &BTreeMap<String, (String, String)>,
    merged: &str,
) -> Result<Vec<String>, PathRefusal> {
    if schema == merged {
        return Ok(Vec::new());
    }
    let along = |side: PathSide<'_>| {
        migration_path(side.chain, migrations, schema).map(|mut path| {
            path.extend(side.migration.map(str::to_owned));
            path
        })
    };
    match version {
        Version::Ours => along(ours),
        Version::Theirs => along(theirs),
        Version::Base => match along(ours) {
            Err(PathRefusal::NoPath) => along(theirs),
            found => found,
        },
    }
}
