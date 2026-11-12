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
