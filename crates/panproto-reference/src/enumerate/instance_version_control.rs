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

/// Every encoding of the scope: each shape with each version below
/// `versions`.
#[must_use]
pub fn encodings(versions: u32) -> Vec<(Shape, u32)> {
    Shape::ALL
        .into_iter()
        .flat_map(|shape| (0..versions).map(move |version| (shape, version)))
        .collect()
}

/// Every combination of payload facts of the scope: each encoding of
/// [`encodings`] with each truth value of the three byte facts.
#[must_use]
pub fn payload_facts(versions: u32) -> Vec<crate::pages::instance_version_control::PayloadFacts> {
    let mut out = Vec::new();
    for (shape, version) in encodings(versions) {
        for bits in 0u8..8 {
            out.push(crate::pages::instance_version_control::PayloadFacts {
                shape,
                version,
                decodes_whole: bits & 1 != 0,
                canonical: bits & 2 != 0,
                count_agrees: bits & 4 != 0,
            });
        }
    }
    out
}

/// Every combination of migration facts of one direction.
///
/// They are each stored state of the data set, (backward) the complement,
/// the migration and the three schemas, with each truth value of the
/// remaining facts the direction reads: `3^5 * 2^3` combinations forward
/// and `3^6 * 2^6` backward.
#[must_use]
pub fn migration_facts(
    backward: bool,
) -> Vec<crate::pages::instance_version_control::MigrationFacts> {
    use crate::pages::instance_version_control::{BackwardFacts, MigrationFacts, Stored};
    const STATES: [Stored; 3] = [Stored::Missing, Stored::WrongKind, Stored::Present];
    let mut sides: Vec<Option<BackwardFacts>> = vec![None];
    if backward {
        sides = Vec::new();
        for complement in STATES {
            for bits in 0u8..8 {
                sides.push(Some(BackwardFacts {
                    complement,
                    names_migration: bits & 1 != 0,
                    shape_agrees: bits & 2 != 0,
                    count_agrees: bits & 4 != 0,
                }));
            }
        }
    }
    let mut out = Vec::new();
    for data in STATES {
        for &side in &sides {
            for migration in STATES {
                for source in STATES {
                    for target in STATES {
                        for data_schema in STATES {
                            for bits in 0u8..8 {
                                out.push(MigrationFacts {
                                    backward: side,
                                    data,
                                    migration,
                                    source,
                                    target,
                                    data_schema,
                                    conforms: bits & 1 != 0,
                                    compiles: bits & 2 != 0,
                                    lens_accepts: bits & 4 != 0,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// The small states of the edit algebra's coverage scope over `schema`.
///
/// They are the states with at most `elements` nodes (graphs and trees) or
/// rows (tables), as instances: graphs, then tables, then single-tree
/// sequences.
#[must_use]
pub fn small_instances(
    schema: &super::edits::CoverageSchema,
    elements: usize,
) -> Vec<crate::pages::instance_version_control::Instance> {
    use crate::pages::instance_version_control::Instance;
    let mut out: Vec<Instance> = super::edits::coverage_graphs(schema)
        .into_iter()
        .filter(|graph| graph.nodes.len() <= elements)
        .map(Instance::Graph)
        .collect();
    out.extend(
        super::edits::coverage_tables(schema)
            .into_iter()
            .filter(|table| table.rows.len() <= elements)
            .map(Instance::Table),
    );
    out.extend(
        super::edits::coverage_trees(schema)
            .into_iter()
            .filter(|tree| tree.nodes.len() <= elements)
            .map(|tree| Instance::Trees(vec![tree])),
    );
    out
}

/// Every commit chain of `commits` commits over `keys`.
///
/// A chain lists, for each commit, oldest first, what it records under each
/// key, in `keys` order: nothing (the key is left out), a fresh data set
/// (`true`) or the end of tracking (`false`). There are
/// `(3^keys)^commits` of them.
#[must_use]
pub fn tracking_chains(commits: usize, keys: &[&str]) -> Vec<Vec<Vec<(String, bool)>>> {
    let mut choices: Vec<Vec<(String, bool)>> = vec![Vec::new()];
    for key in keys {
        choices = choices
            .into_iter()
            .flat_map(|prefix| {
                [None, Some(true), Some(false)]
                    .into_iter()
                    .map(move |choice| {
                        let mut grown = prefix.clone();
                        if let Some(record) = choice {
                            grown.push(((*key).to_owned(), record));
                        }
                        grown
                    })
            })
            .collect();
    }
    let mut out = vec![Vec::new()];
    for _ in 0..commits {
        out = out
            .into_iter()
            .flat_map(|prefix: Vec<Vec<(String, bool)>>| {
                choices.iter().map(move |choice| {
                    let mut grown = prefix.clone();
                    grown.push(choice.clone());
                    grown
                })
            })
            .collect();
    }
    out
}

/// The states of the instance merge scope over `schema`, by shape.
///
/// They are the graphs with at most one node and the tables with at most
/// one row of the edit algebra's coverage scope, and the sequences of at
/// most two trees drawn from the one-node trees of that scope.
#[must_use]
pub fn merge_states(
    schema: &super::edits::CoverageSchema,
) -> Vec<Vec<crate::pages::instance_version_control::Instance>> {
    use crate::pages::instance_version_control::Instance;
    let graphs = super::edits::coverage_graphs(schema)
        .into_iter()
        .filter(|graph| graph.nodes.len() <= 1)
        .map(Instance::Graph)
        .collect();
    let tables = super::edits::coverage_tables(schema)
        .into_iter()
        .filter(|table| table.rows.len() <= 1)
        .map(Instance::Table)
        .collect();
    let single: Vec<crate::model::Tree> = super::edits::coverage_trees(schema)
        .into_iter()
        .filter(|tree| tree.nodes.len() == 1)
        .collect();
    let mut sequences = vec![Instance::Trees(Vec::new())];
    sequences.extend(
        single
            .iter()
            .map(|tree| Instance::Trees(vec![tree.clone()])),
    );
    for first in &single {
        for second in &single {
            sequences.push(Instance::Trees(vec![first.clone(), second.clone()]));
        }
    }
    vec![graphs, tables, sequences]
}

/// What a commit of the path scope records about its first parent's
/// schema: no migration, the migration from its parent's schema to its own,
/// or the migration the other way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Link {
    /// No migration.
    Missing,
    /// The migration from the parent's schema to the commit's.
    Forward,
    /// The migration from the commit's schema to the parent's.
    Reverse,
}

impl Link {
    /// Every link, in declaration order.
    pub const ALL: [Self; 3] = [Self::Missing, Self::Forward, Self::Reverse];
}

/// Every first-parent chain of the path scope with `commits` commits,
/// oldest first: each commit's schema (an index below `schemas`) and, for
/// each commit but the root, its link. There are
/// `schemas * (3 * schemas)^(commits - 1)` of them.
#[must_use]
pub fn path_chains(commits: usize, schemas: usize) -> Vec<Vec<(usize, Link)>> {
    if commits == 0 {
        return Vec::new();
    }
    let mut out: Vec<Vec<(usize, Link)>> = (0..schemas)
        .map(|schema| vec![(schema, Link::Missing)])
        .collect();
    for _ in 1..commits {
        out = out
            .into_iter()
            .flat_map(|prefix| {
                (0..schemas).flat_map(move |schema| {
                    let prefix = prefix.clone();
                    Link::ALL.into_iter().map(move |link| {
                        let mut grown = prefix.clone();
                        grown.push((schema, link));
                        grown
                    })
                })
            })
            .collect();
    }
    out
}

/// The count of [`path_chains`], by its closed formula.
#[must_use]
pub fn path_chains_count(commits: u32, schemas: u64) -> u64 {
    if commits == 0 {
        return 0;
    }
    schemas * (3 * schemas).pow(commits - 1)
}
