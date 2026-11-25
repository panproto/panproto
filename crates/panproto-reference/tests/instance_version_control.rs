//! The instance version control reference on hand-computed cases.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::enumerate::instance_version_control::{
    GC_CASES_COUNT, GC_KEPT_TOTAL, OBJECTS_PER_REPOSITORY, Roots, gc_cases, repository,
};
use panproto_reference::pages::instance_version_control::{Head, Object, Refusal, Repository};

fn names(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn reachability_on_hand_computed_repositories() {
    let bare = repository([[0, 0, 0], [0, 0, 0]], Roots::MainAtFirst);
    assert_eq!(bare.reachable(), names(&["C1", "S", "S.leaf"]));

    // The first commit records the tree data set with its complement, the
    // second the graph data set with its edit log; the stash reflog alone
    // names the second commit.
    let stashed = repository([[2, 0, 0], [0, 0, 3]], Roots::StashLog);
    assert_eq!(
        stashed.reachable(),
        names(&[
            "C1", "C2", "S", "S.leaf", "M", "F0", "F1", "D.tree", "K.tree", "D.graph", "K.graph",
            "L.graph",
        ])
    );
    assert!(stashed.garbage().contains("E.tree"));
    assert!(stashed.garbage().contains("G.graph"));

    let staged = repository([[3, 3, 3], [3, 3, 3]], Roots::StagedOnly);
    assert_eq!(staged.reachable(), names(&["D.graph", "S", "S.leaf"]));

    let none = repository([[3, 3, 3], [3, 3, 3]], Roots::None);
    assert_eq!(none.reachable(), BTreeSet::new());
    assert_eq!(none.garbage().len(), OBJECTS_PER_REPOSITORY);

    let tagged = repository([[0, 0, 0], [1, 0, 0]], Roots::AnnotatedTag);
    assert_eq!(
        tagged.reachable(),
        names(&["T", "C2", "C1", "S", "S.leaf", "M", "F0", "F1", "D.tree"])
    );
}

#[test]
fn a_missing_name_is_not_followed_and_a_cycle_ends() {
    let mut objects = BTreeMap::new();
    objects.insert(
        "a".to_owned(),
        Object::Tag {
            target: "b".to_owned(),
        },
    );
    objects.insert(
        "b".to_owned(),
        Object::Tag {
            target: "a".to_owned(),
        },
    );
    objects.insert(
        "c".to_owned(),
        Object::Tag {
            target: "missing".to_owned(),
        },
    );
    let repo = Repository {
        objects,
        refs: BTreeMap::from([
            ("refs/heads/x".to_owned(), "a".to_owned()),
            ("refs/heads/y".to_owned(), "c".to_owned()),
        ]),
        head: Head::Branch("main".to_owned()),
        stash_log: Vec::new(),
        staged: Vec::new(),
    };
    assert_eq!(repo.reachable(), names(&["a", "b", "c"]));
    assert_eq!(repo.garbage(), BTreeSet::new());
}

#[test]
fn the_gc_scope_has_its_closed_form_size_and_kept_total() {
    let cases = gc_cases();
    assert_eq!(cases.len(), GC_CASES_COUNT);
    assert_eq!(GC_CASES_COUNT, 36_864);
    assert!(
        cases
            .iter()
            .all(|case| case.repository.objects.len() == OBJECTS_PER_REPOSITORY)
    );
    let kept: usize = cases
        .iter()
        .map(|case| case.repository.reachable().len())
        .sum();
    assert_eq!(kept, GC_KEPT_TOTAL);
    assert_eq!(GC_KEPT_TOTAL, 359_936);
}

#[test]
fn tracking_and_diffs_on_hand_computed_cases() -> Result<(), Refusal> {
    use panproto_reference::model::{Graph, Node, Table};
    use panproto_reference::pages::instance_version_control::{
        Change, Entry, Instance, apply_delta, diff_instances, diff_tracked, tracked,
    };

    let graph = |ids: &[u32]| {
        Instance::Graph(Graph {
            nodes: ids.iter().map(|id| (*id, Node::new(*id, "a"))).collect(),
            edges: BTreeMap::new(),
        })
    };
    let entry = |data: &str, schema: &str, instance: Instance| Entry {
        data: data.to_owned(),
        schema: schema.to_owned(),
        instance,
    };

    // The nearer commit's entry wins; a key recorded only further back is
    // still tracked.
    let newest = BTreeMap::from([("k".to_owned(), Some(entry("d2", "s", graph(&[0, 1]))))]);
    let oldest = BTreeMap::from([
        ("k".to_owned(), Some(entry("d1", "s", graph(&[0])))),
        ("j".to_owned(), Some(entry("d0", "s", graph(&[])))),
    ]);
    let at_newest = tracked(&[newest.clone(), oldest.clone()]);
    assert_eq!(at_newest["k"].data, "d2");
    assert_eq!(at_newest["j"].data, "d0");

    // A nearer commit that stops tracking `j` hides the older entry, and a
    // still nearer one records it again.
    let stopped = BTreeMap::from([("j".to_owned(), None)]);
    let without_j = tracked(&[stopped.clone(), newest.clone(), oldest.clone()]);
    assert_eq!(without_j.keys().collect::<Vec<_>>(), vec!["k"]);
    let again = BTreeMap::from([("j".to_owned(), Some(entry("d4", "s", graph(&[2]))))]);
    assert_eq!(
        tracked(&[again, stopped, newest, oldest.clone()])["j"].data,
        "d4"
    );

    // Diffing the oldest against the newest changes `k` by adding node 1,
    // and applying the delta gives the newer graph.
    let at_oldest = tracked(&[oldest]);
    let changes = diff_tracked(&at_oldest, &at_newest)?;
    let [Change::Changed(key, delta)] = changes.as_slice() else {
        panic!("one change expected, got {changes:?}");
    };
    assert_eq!(key, "k");
    assert_eq!(apply_delta(delta, &graph(&[0]))?, graph(&[0, 1]));

    // Shapes and schemas.
    assert_eq!(
        diff_instances(&graph(&[]), &Instance::Table(Table::default())),
        Err(Refusal::ShapeMismatch)
    );
    let other_schema = BTreeMap::from([("k".to_owned(), entry("d3", "t", graph(&[0])))]);
    assert_eq!(
        diff_tracked(&at_newest, &other_schema),
        Err(Refusal::SchemaMismatch("k".to_owned()))
    );
    let removed = diff_tracked(&at_newest, &BTreeMap::new())?;
    assert_eq!(
        removed,
        vec![
            Change::Removed("j".to_owned()),
            Change::Removed("k".to_owned())
        ]
    );
    Ok(())
}

#[test]
fn instance_merges_on_hand_computed_cases() -> Result<(), Refusal> {
    use panproto_reference::edit::{EditConflict, GElem};
    use panproto_reference::model::{Graph, Node, Tree};
    use panproto_reference::pages::instance_version_control::{
        Conflict, Entry, Instance, Merged, RecordConflict, Side, TrackingConflict, merge_tracked,
    };

    let graph = |ids: &[u32]| Graph {
        nodes: ids.iter().map(|id| (*id, Node::new(*id, "a"))).collect(),
        edges: BTreeMap::new(),
    };
    let entry = |data: &str, schema: &str, instance: Instance| Entry {
        data: data.to_owned(),
        schema: schema.to_owned(),
        instance,
    };
    let one = |key: &str, e: Entry| BTreeMap::from([(key.to_owned(), e)]);

    // Ours adds node 1 and theirs node 2: independent, so the merge holds
    // both.
    let base = one("k", entry("b", "s", Instance::Graph(graph(&[0]))));
    let ours = one("k", entry("o", "s", Instance::Graph(graph(&[0, 1]))));
    let theirs = one("k", entry("t", "s", Instance::Graph(graph(&[0, 2]))));
    let (merged, conflicts) = merge_tracked(&base, &ours, &theirs)?;
    assert_eq!(conflicts, vec![]);
    assert_eq!(
        merged["k"],
        Merged::New("s".to_owned(), Instance::Graph(graph(&[0, 1, 2])))
    );

    // Both add node 1: one `AddAdd` conflict, nothing merged.
    let theirs_too = one("k", entry("t", "s", Instance::Graph(graph(&[0, 1]))));
    let (merged, conflicts) = merge_tracked(&base, &ours, &theirs_too)?;
    assert_eq!(merged.len(), 0);
    assert_eq!(
        conflicts,
        vec![Conflict::GraphEdit(
            "k".to_owned(),
            None,
            EditConflict::AddAdd {
                elem: GElem::Node(1)
            }
        )]
    );

    // One side unchanged takes the other's data set; both sides adding the
    // key differently, removing and changing, and mixed schemas conflict.
    let (merged, _) = merge_tracked(&base, &base, &theirs)?;
    assert_eq!(merged["k"], Merged::Kept("t".to_owned()));
    let (_, conflicts) = merge_tracked(&BTreeMap::new(), &ours, &theirs)?;
    assert_eq!(
        conflicts,
        vec![Conflict::Tracking(
            "k".to_owned(),
            TrackingConflict::AddedOnBothSides
        )]
    );
    let (_, conflicts) = merge_tracked(&base, &BTreeMap::new(), &theirs)?;
    assert_eq!(
        conflicts,
        vec![Conflict::Tracking(
            "k".to_owned(),
            TrackingConflict::RemovedAndChanged(Side::Ours)
        )]
    );
    let elsewhere = one("k", entry("t", "s2", Instance::Graph(graph(&[0, 2]))));
    let (_, conflicts) = merge_tracked(&base, &ours, &elsewhere)?;
    assert_eq!(
        conflicts,
        vec![Conflict::Tracking(
            "k".to_owned(),
            TrackingConflict::SchemaDiffers
        )]
    );

    // Trees: one side appends a record and the other removes the last one,
    // which the appending side left unchanged; both appending conflicts.
    let tree = |root: u32| Tree {
        nodes: BTreeMap::from([(root, Node::new(root, "a"))]),
        root,
        arcs: Vec::new(),
        fans: Vec::new(),
    };
    let trees = |roots: &[u32]| Instance::Trees(roots.iter().map(|r| tree(*r)).collect());
    let base = one("k", entry("b", "s", trees(&[0, 1])));
    let ours = one("k", entry("o", "s", trees(&[0])));
    let theirs = one("k", entry("t", "s", trees(&[0, 1, 2])));
    let (merged, conflicts) = merge_tracked(&base, &ours, &theirs)?;
    assert_eq!(conflicts, vec![]);
    assert_eq!(merged["k"], Merged::New("s".to_owned(), trees(&[0, 2])));
    let ours_appends = one("k", entry("o", "s", trees(&[0, 1, 3])));
    let (_, conflicts) = merge_tracked(&base, &ours_appends, &theirs)?;
    assert_eq!(
        conflicts,
        vec![Conflict::Record(
            "k".to_owned(),
            2,
            RecordConflict::AppendedByBoth
        )]
    );
    Ok(())
}
