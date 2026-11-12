//! The instance version control reference on hand-computed cases.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::enumerate::instance_version_control::{
    GC_CASES_COUNT, GC_KEPT_TOTAL, OBJECTS_PER_REPOSITORY, Roots, gc_cases, repository,
};
use panproto_reference::pages::instance_version_control::{Head, Object, Repository};

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
