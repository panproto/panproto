//! Three-way schema merge, the reference for the Schema merge correctness
//! page.
//!
//! A schema is its vertex ids with their kinds and its set of directed edges,
//! each named by its endpoint pair. The merge of a base `B` and two branches
//! `O` and `T` is read off one element at a time from the element's value in
//! each of the three schemas (a kind or absence for a vertex, presence or
//! absence for an edge), by the rule the page states: a value both branches
//! agree on is taken, a value one branch left as it was in the base gives
//! way to the other branch's, and three distinct values are a conflict.
//! Deleting a vertex conflicts with the other branch adding an edge at it.
//! The definitions read no diff and share no code with production.

use std::collections::{BTreeMap, BTreeSet};

/// A directed edge, named by its source and target vertex ids.
pub type Edge = (String, String);

/// A schema of the merge scope.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Schema {
    /// Vertex ids with their kinds.
    pub vertices: BTreeMap<String, String>,
    /// The edges.
    pub edges: BTreeSet<Edge>,
}

impl Schema {
    /// Whether every edge's endpoints are vertices of the schema.
    #[must_use]
    pub fn well_formed(&self) -> bool {
        self.edges
            .iter()
            .all(|(src, tgt)| self.vertices.contains_key(src) && self.vertices.contains_key(tgt))
    }
}

/// The branch that made a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    /// The first branch, `O`.
    Ours,
    /// The second branch, `T`.
    Theirs,
}

/// A conflict on one vertex.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Conflict {
    /// Both branches changed the base kind, to different kinds.
    BothModifiedVertex {
        /// The vertex.
        vertex: String,
        /// The kind the first branch gave it.
        ours: String,
        /// The kind the second branch gave it.
        theirs: String,
    },
    /// Both branches added the vertex, with different kinds.
    BothAddedVertexDifferently {
        /// The vertex.
        vertex: String,
        /// The kind the first branch gave it.
        ours: String,
        /// The kind the second branch gave it.
        theirs: String,
    },
    /// One branch deleted the vertex and the other changed its kind or added
    /// an edge at it.
    DeleteModifyVertex {
        /// The vertex.
        vertex: String,
        /// The branch that deleted it.
        deleted_by: Side,
    },
}

impl Conflict {
    /// The key conflicts are listed in ascending order of: the class
    /// (both modified, both added, delete against modify or use), then the
    /// vertex id.
    #[must_use]
    pub fn order_key(&self) -> (u8, &str) {
        match self {
            Self::BothModifiedVertex { vertex, .. } => (0, vertex),
            Self::BothAddedVertexDifferently { vertex, .. } => (1, vertex),
            Self::DeleteModifyVertex { vertex, .. } => (2, vertex),
        }
    }

    /// The conflict with the two branches exchanged.
    #[must_use]
    pub fn swapped(&self) -> Self {
        match self {
            Self::BothModifiedVertex {
                vertex,
                ours,
                theirs,
            } => Self::BothModifiedVertex {
                vertex: vertex.clone(),
                ours: theirs.clone(),
                theirs: ours.clone(),
            },
            Self::BothAddedVertexDifferently {
                vertex,
                ours,
                theirs,
            } => Self::BothAddedVertexDifferently {
                vertex: vertex.clone(),
                ours: theirs.clone(),
                theirs: ours.clone(),
            },
            Self::DeleteModifyVertex { vertex, deleted_by } => Self::DeleteModifyVertex {
                vertex: vertex.clone(),
                deleted_by: match deleted_by {
                    Side::Ours => Side::Theirs,
                    Side::Theirs => Side::Ours,
                },
            },
        }
    }
}

/// An element both branches added with the same value, kept once.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Identified {
    /// A vertex both branches added with the same kind.
    Vertex(String),
    /// An edge both branches added.
    Edge(Edge),
}

/// What the merge of a base and two branches is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merge {
    /// The merged schema; a conflicted vertex has its base value.
    pub merged: Schema,
    /// The conflicts, in ascending [`Conflict::order_key`] order.
    pub conflicts: Vec<Conflict>,
    /// The identified additions: vertices by id, then edges by endpoint pair.
    pub identified: Vec<Identified>,
}

/// Whether `side` holds an edge at `vertex` that `base` does not.
fn adds_edge_at(base: &Schema, side: &Schema, vertex: &str) -> bool {
    side.edges
        .iter()
        .filter(|edge| !base.edges.contains(*edge))
        .any(|(src, tgt)| src == vertex || tgt == vertex)
}

/// The merge of `ours` and `theirs` over `base`, by the per-element rule.
#[must_use]
pub fn merge(base: &Schema, ours: &Schema, theirs: &Schema) -> Merge {
    let mut merged = Schema::default();
    let mut conflicts = Vec::new();
    let mut identified = Vec::new();

    let ids: BTreeSet<&String> = base
        .vertices
        .keys()
        .chain(ours.vertices.keys())
        .chain(theirs.vertices.keys())
        .collect();
    for id in ids {
        let (b, o, t) = (
            base.vertices.get(id),
            ours.vertices.get(id),
            theirs.vertices.get(id),
        );
        let taken = if o == t {
            if b.is_none() && o.is_some() {
                identified.push(Identified::Vertex(id.clone()));
            }
            o
        } else if o == b {
            if t.is_none() && adds_edge_at(base, ours, id) {
                conflicts.push(Conflict::DeleteModifyVertex {
                    vertex: id.clone(),
                    deleted_by: Side::Theirs,
                });
                b
            } else {
                t
            }
        } else if t == b {
            if o.is_none() && adds_edge_at(base, theirs, id) {
                conflicts.push(Conflict::DeleteModifyVertex {
                    vertex: id.clone(),
                    deleted_by: Side::Ours,
                });
                b
            } else {
                o
            }
        } else {
            conflicts.push(match (b, o, t) {
                (None, Some(ours), Some(theirs)) => Conflict::BothAddedVertexDifferently {
                    vertex: id.clone(),
                    ours: ours.clone(),
                    theirs: theirs.clone(),
                },
                (Some(_), Some(ours), Some(theirs)) => Conflict::BothModifiedVertex {
                    vertex: id.clone(),
                    ours: ours.clone(),
                    theirs: theirs.clone(),
                },
                (_, None, _) => Conflict::DeleteModifyVertex {
                    vertex: id.clone(),
                    deleted_by: Side::Ours,
                },
                (_, _, None) => Conflict::DeleteModifyVertex {
                    vertex: id.clone(),
                    deleted_by: Side::Theirs,
                },
            });
            b
        };
        if let Some(kind) = taken {
            merged.vertices.insert(id.clone(), kind.clone());
        }
    }

    let edges: BTreeSet<&Edge> = base
        .edges
        .iter()
        .chain(&ours.edges)
        .chain(&theirs.edges)
        .collect();
    for edge in edges {
        let (b, o, t) = (
            base.edges.contains(edge),
            ours.edges.contains(edge),
            theirs.edges.contains(edge),
        );
        if o && t && !b {
            identified.push(Identified::Edge(edge.clone()));
        }
        // Presence has two values, so two of the three always agree and an
        // edge never conflicts: both branches agreeing, or one branch
        // leaving the base value, gives the other branch's.
        let kept = if o == b { t } else { o };
        if kept {
            merged.edges.insert(edge.clone());
        }
    }

    conflicts.sort_by(|left, right| left.order_key().cmp(&right.order_key()));
    identified.sort();
    Merge {
        merged,
        conflicts,
        identified,
    }
}

/// The vertex and edge maps a staged change selects when no vertex is both
/// removed and added: the identity on the vertices and on the edges the two
/// schemas share.
#[must_use]
pub fn surviving_identity(
    old: &Schema,
    new: &Schema,
) -> (BTreeMap<String, String>, BTreeSet<Edge>) {
    let vertices = old
        .vertices
        .keys()
        .filter(|id| new.vertices.contains_key(*id))
        .map(|id| (id.clone(), id.clone()))
        .collect();
    let edges = old.edges.intersection(&new.edges).cloned().collect();
    (vertices, edges)
}

/// Whether a change from `old` to `new` both removes and adds a vertex, the
/// one case in which a rename may be selected.
#[must_use]
pub fn may_rename(old: &Schema, new: &Schema) -> bool {
    old.vertices.keys().any(|id| !new.vertices.contains_key(id))
        && new.vertices.keys().any(|id| !old.vertices.contains_key(id))
}

/// The repository format version this reference reads.
pub const FORMAT_VERSION: u32 = 1;

/// The repository format version a `.panproto/format` marker names, or
/// `None` when the marker is refused.
///
/// An absent marker is version 1; a
/// present one is accepted only when it is the decimal numeral of
/// [`FORMAT_VERSION`] followed by one line feed.
#[must_use]
pub fn repository_format(marker: Option<&[u8]>) -> Option<u32> {
    match marker {
        None => Some(1),
        Some(bytes) => {
            let text = std::str::from_utf8(bytes).ok()?;
            let numeral = text.strip_suffix('\n')?;
            let canonical = FORMAT_VERSION.to_string();
            (numeral == canonical).then_some(FORMAT_VERSION)
        }
    }
}

/// The canonical rendering of a merge: the merged vertices and edges, the
/// conflicts in their order and the identified additions, one line.
#[must_use]
pub fn render(merge: &Merge) -> String {
    let conflicts: Vec<String> = merge
        .conflicts
        .iter()
        .map(|conflict| match conflict {
            Conflict::BothModifiedVertex {
                vertex,
                ours,
                theirs,
            } => format!("modified {vertex} {ours} {theirs}"),
            Conflict::BothAddedVertexDifferently {
                vertex,
                ours,
                theirs,
            } => format!("added {vertex} {ours} {theirs}"),
            Conflict::DeleteModifyVertex { vertex, deleted_by } => {
                format!("deleted {vertex} {deleted_by:?}")
            }
        })
        .collect();
    format!(
        "{:?}|{:?}|{conflicts:?}|{:?}",
        merge.merged.vertices, merge.merged.edges, merge.identified
    )
}
