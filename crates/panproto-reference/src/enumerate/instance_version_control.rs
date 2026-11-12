//! The complete finite scope of the instance version control page.

use std::collections::BTreeMap;

use crate::pages::instance_version_control::{Head, Object, Repository, Shape};

/// How much of one shape's objects a commit records: nothing, its data
/// set, its data set and complement, or those and its edit log.
pub const LEVELS: u8 = 4;

/// The roots a repository of the scope has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Roots {
    /// No ref; `HEAD` on the unborn branch `main`.
    None,
    /// `refs/heads/main` names the first commit, and `HEAD` is on `main`.
    MainAtFirst,
    /// `refs/heads/main` names the second commit, and `HEAD` is on `main`.
    MainAtSecond,
    /// No ref; `HEAD` detached at the first commit.
    DetachedAtFirst,
    /// `refs/tags/v1` names the annotated tag of the second commit.
    AnnotatedTag,
    /// `refs/remotes/origin/main` names the first commit.
    RemoteAtFirst,
    /// `refs/notes/review`, a namespace no command writes, names the second
    /// commit.
    OtherNamespace,
    /// `refs/stash` names the first commit, and the stash's reflog names the
    /// second commit and then the first.
    StashLog,
    /// No ref; the staging index names the graph data set.
    StagedOnly,
}

impl Roots {
    /// Every root configuration, in declaration order.
    pub const ALL: [Self; 9] = [
        Self::None,
        Self::MainAtFirst,
        Self::MainAtSecond,
        Self::DetachedAtFirst,
        Self::AnnotatedTag,
        Self::RemoteAtFirst,
        Self::OtherNamespace,
        Self::StashLog,
        Self::StagedOnly,
    ];
}

/// The name of a shape in object names.
#[must_use]
pub const fn shape_name(shape: Shape) -> &'static str {
    match shape {
        Shape::Tree => "tree",
        Shape::Table => "table",
        Shape::Graph => "graph",
    }
}

/// A repository of the scope with the choices that made it.
#[derive(Clone, Debug)]
pub struct GcCase {
    /// The level each commit records each shape at, in [`Shape::ALL`]
    /// order: `levels[0]` for `C1`, `levels[1]` for `C2`.
    pub levels: [[u8; 3]; 2],
    /// The roots.
    pub roots: Roots,
    /// The repository.
    pub repository: Repository,
}

fn named(name: &str) -> String {
    name.to_owned()
}

/// The objects every repository of the scope stores besides its commits.
///
/// They are the schema tree `S` over the leaf `S.leaf`, the migration `M` between
/// the flat schemas `F0` and `F1`, and for each shape `x` the data set
/// `D.x`, its complement `K.x` under `M`, the edit log `L.x` over both, an
/// orphan data set `G.x` and an orphan edit log `E.x` over `G.x` and `K.x`.
#[must_use]
pub fn fixed_objects() -> BTreeMap<String, Object> {
    let mut objects = BTreeMap::new();
    objects.insert(
        named("S"),
        Object::SchemaTree {
            entries: vec![named("S.leaf")],
        },
    );
    for leaf in ["S.leaf", "F0", "F1"] {
        objects.insert(named(leaf), Object::Leaf);
    }
    objects.insert(
        named("M"),
        Object::Migration {
            src: named("F0"),
            tgt: named("F1"),
        },
    );
    for shape in Shape::ALL {
        let x = shape_name(shape);
        let (data, complement, orphan) = (format!("D.{x}"), format!("K.{x}"), format!("G.{x}"));
        objects.insert(
            data.clone(),
            Object::DataSet {
                shape,
                schema: named("S"),
            },
        );
        objects.insert(
            complement.clone(),
            Object::Complement {
                shape,
                migration: named("M"),
                data: data.clone(),
            },
        );
        objects.insert(
            format!("L.{x}"),
            Object::EditLog {
                shape,
                schema: named("S"),
                data,
                final_complement: complement.clone(),
            },
        );
        objects.insert(
            orphan.clone(),
            Object::DataSet {
                shape,
                schema: named("S"),
            },
        );
        objects.insert(
            format!("E.{x}"),
            Object::EditLog {
                shape,
                schema: named("S"),
                data: orphan,
                final_complement: complement,
            },
        );
    }
    objects
}

/// A commit recording each shape at its level.
fn commit(levels: [u8; 3], migration: Option<&str>, parents: &[&str]) -> Object {
    let mut data = Vec::new();
    let mut complements = Vec::new();
    let mut edit_logs = Vec::new();
    for (shape, level) in Shape::ALL.into_iter().zip(levels) {
        let x = shape_name(shape);
        if level >= 1 {
            data.push(format!("D.{x}"));
        }
        if level >= 2 {
            complements.push(format!("K.{x}"));
        }
        if level >= 3 {
            edit_logs.push(format!("L.{x}"));
        }
    }
    Object::Commit {
        schema: named("S"),
        migration: migration.map(str::to_owned),
        parents: parents.iter().map(|p| (*p).to_owned()).collect(),
        data,
        complements,
        edit_logs,
        metadata: Vec::new(),
    }
}

/// The repository with the given commit levels and roots.
#[must_use]
pub fn repository(levels: [[u8; 3]; 2], roots: Roots) -> Repository {
    let mut objects = fixed_objects();
    objects.insert(named("C1"), commit(levels[0], None, &[]));
    objects.insert(named("C2"), commit(levels[1], Some("M"), &["C1"]));
    objects.insert(
        named("T"),
        Object::Tag {
            target: named("C2"),
        },
    );
    let mut refs = BTreeMap::new();
    let mut head = Head::Branch(named("main"));
    let mut stash_log = Vec::new();
    let mut staged = Vec::new();
    match roots {
        Roots::None => {}
        Roots::MainAtFirst => {
            refs.insert(named("refs/heads/main"), named("C1"));
        }
        Roots::MainAtSecond => {
            refs.insert(named("refs/heads/main"), named("C2"));
        }
        Roots::DetachedAtFirst => head = Head::Detached(named("C1")),
        Roots::AnnotatedTag => {
            refs.insert(named("refs/tags/v1"), named("T"));
        }
        Roots::RemoteAtFirst => {
            refs.insert(named("refs/remotes/origin/main"), named("C1"));
        }
        Roots::OtherNamespace => {
            refs.insert(named("refs/notes/review"), named("C2"));
        }
        Roots::StashLog => {
            refs.insert(named("refs/stash"), named("C1"));
            stash_log = vec![named("C2"), named("C1")];
        }
        Roots::StagedOnly => staged.push(named("D.graph")),
    }
    Repository {
        objects,
        refs,
        head,
        stash_log,
        staged,
    }
}

/// Every repository of the scope: each root configuration of
/// [`Roots::ALL`] with each choice of a level below [`LEVELS`] for each
/// shape at each of the two commits.
#[must_use]
pub fn gc_cases() -> Vec<GcCase> {
    let level_triples: Vec<[u8; 3]> = (0..LEVELS)
        .flat_map(|a| (0..LEVELS).flat_map(move |b| (0..LEVELS).map(move |c| [a, b, c])))
        .collect();
    let mut out = Vec::new();
    for roots in Roots::ALL {
        for first in &level_triples {
            for second in &level_triples {
                let levels = [*first, *second];
                out.push(GcCase {
                    levels,
                    roots,
                    repository: repository(levels, roots),
                });
            }
        }
    }
    out
}

/// The number of repositories [`gc_cases`] lists: nine root
/// configurations times 64 level choices for each of two commits.
pub const GC_CASES_COUNT: usize = 9 * 64 * 64;

/// The number of objects every repository of the scope stores: `S`,
/// `S.leaf`, `F0`, `F1`, `M`, five objects of each shape, the two commits
/// and the tag.
pub const OBJECTS_PER_REPOSITORY: usize = 5 + 5 * 3 + 3;

/// The total, over the scope, of the number of objects garbage collection
/// keeps, counted without enumerating repositories.
///
/// A repository whose roots reach only `C1` (main, detached `HEAD` or the
/// remote ref at the first commit) keeps `C1`, `S` and `S.leaf`, the
/// `l` objects of each shape at level `l`, and `M`, `F0` and `F1` when some
/// shape is at level 2 or more. Over the 64 level choices of `C1` the shape
/// objects sum to `3 * 16 * (0 + 1 + 2 + 3) = 288` and 56 choices have a
/// complement, so the sum is `64 * 3 + 288 + 56 * 3 = 648`, for each of the
/// 64 choices of `C2`: 41,472. A repository whose roots reach `C2` (main at
/// the second commit, the other namespace, the stash log, the tag) keeps
/// `C2`, `C1`, `S`, `S.leaf`, `M`, `F0` and `F1`, and of each shape the
/// objects of the higher of its two levels; the higher of two levels sums
/// to `0 + 3 + 10 + 21 = 34` over the 16 pairs, so the sum is
/// `7 * 4096 + 3 * 34 * 256 = 54,784`, and 58,880 with the tag. The staged
/// graph data set keeps itself, `S` and `S.leaf`: `3 * 4096 = 12,288`. No
/// root keeps nothing. The total is `3 * 41,472 + 3 * 54,784 + 58,880 +
/// 12,288`.
pub const GC_KEPT_TOTAL: usize = 3 * 41_472 + 3 * 54_784 + 58_880 + 12_288;
