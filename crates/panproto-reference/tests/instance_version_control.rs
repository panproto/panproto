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
    let newest = BTreeMap::from([("k".to_owned(), entry("d2", "s", graph(&[0, 1])))]);
    let oldest = BTreeMap::from([
        ("k".to_owned(), entry("d1", "s", graph(&[0]))),
        ("j".to_owned(), entry("d0", "s", graph(&[]))),
    ]);
    let at_newest = tracked(&[newest, oldest.clone()]);
    assert_eq!(at_newest["k"].data, "d2");
    assert_eq!(at_newest["j"].data, "d0");

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
