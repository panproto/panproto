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

/// The entries a commit tracks by key, given the entries each commit of
/// its first-parent chain records, the commit itself first: for each key,
/// the entry of the first commit that records one.
#[must_use]
pub fn tracked(chain: &[BTreeMap<String, Entry>]) -> BTreeMap<String, Entry> {
    let mut out = BTreeMap::new();
    for recorded in chain {
        for (key, entry) in recorded {
            out.entry(key.clone()).or_insert_with(|| entry.clone());
        }
    }
    out
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
