//! Comprehensive integration tests for panproto-vcs.
//!
//! These tests exercise the full public API through filesystem-backed
//! repositories created in temporary directories.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::{Constraint, Edge, Schema, Vertex};
use panproto_vcs::dag;
use panproto_vcs::merge::{MergeConflict, MergeOptions, Side};
use panproto_vcs::reset::ResetMode;
use panproto_vcs::store::{self, HeadState};
use panproto_vcs::{ObjectId, Repository, Store, VcsError, refs};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_schema(vertices: &[(&str, &str)]) -> Schema {
    let mut vert_map = HashMap::new();
    for (id, kind) in vertices {
        vert_map.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }
    Schema {
        protocol: "test".into(),
        vertices: vert_map,
        edges: HashMap::new(),
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing: HashMap::new(),
        incoming: HashMap::new(),
        between: HashMap::new(),
    }
}

fn make_schema_with_edges(vertices: &[(&str, &str)], edges: &[(&str, &str, &str)]) -> Schema {
    let mut s = make_schema(vertices);
    for (src, tgt, kind) in edges {
        let edge = Edge {
            src: (*src).into(),
            tgt: (*tgt).into(),
            kind: Name::from(*kind),
            name: None,
        };
        s.edges.insert(edge, Name::from(*kind));
    }
    s
}

/// Build a schema with named edges (prop edges with a `name` field).
///
/// Each edge tuple is `(src, tgt, kind, name)`.
fn make_schema_with_named_edges(
    vertices: &[(&str, &str)],
    edges: &[(&str, &str, &str, &str)],
) -> Schema {
    let mut s = make_schema(vertices);
    for (src, tgt, kind, name) in edges {
        let edge = Edge {
            src: (*src).into(),
            tgt: (*tgt).into(),
            kind: Name::from(*kind),
            name: Some(Name::from(*name)),
        };
        s.edges.insert(edge.clone(), Name::from(*kind));
        s.outgoing
            .entry(Name::from(*src))
            .or_default()
            .push(edge.clone());
        s.incoming
            .entry(Name::from(*tgt))
            .or_default()
            .push(edge.clone());
        s.between
            .entry((Name::from(*src), Name::from(*tgt)))
            .or_default()
            .push(edge);
    }
    s
}

fn make_schema_with_constraints(
    vertices: &[(&str, &str)],
    constraints: &[(&str, &str, &str)],
) -> Schema {
    let mut s = make_schema(vertices);
    for (vid, sort, value) in constraints {
        s.constraints
            .entry(Name::from(*vid))
            .or_default()
            .push(Constraint {
                sort: Name::from(*sort),
                value: value.to_string(),
            });
    }
    s
}

/// Helper: init repo, add schema, commit, return (repo, `ObjectId`).
fn init_with_schema(
    dir: &std::path::Path,
    vertices: &[(&str, &str)],
    msg: &str,
    author: &str,
) -> Result<(Repository, ObjectId), Box<dyn std::error::Error>> {
    let mut repo = Repository::init(dir)?;
    let s = make_schema(vertices);
    repo.add(&s)?;
    let cid = repo.commit(msg, author)?;
    Ok((repo, cid))
}

// ===========================================================================
// Group 1: Repository Lifecycle
// ===========================================================================

#[test]
fn init_creates_panproto_dir() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let _repo = Repository::init(dir.path())?;
    assert!(dir.path().join(".panproto").exists());
    assert!(dir.path().join(".panproto/objects").exists());
    assert!(dir.path().join(".panproto/refs/heads").exists());
    Ok(())
}

#[test]
fn open_nonexistent_fails() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let result = Repository::open(dir.path());
    assert!(result.is_err());
    Ok(())
}

#[test]
fn double_init_fails() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let _repo = Repository::init(dir.path())?;
    // Re-initializing overwrites HEAD; verify it resets cleanly.
    let repo2 = Repository::init(dir.path())?;
    assert_eq!(repo2.store().get_head()?, HeadState::Branch("main".into()));
    // Log should fail because the re-init cleared the branch ref.
    assert!(repo2.log(None).is_err());
    Ok(())
}

#[test]
fn empty_repo_log_fails() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let repo = Repository::init(dir.path())?;
    let result = repo.log(None);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn commit_without_add_fails() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;
    let result = repo.commit("empty", "alice");
    assert!(matches!(result, Err(VcsError::NothingStaged)));
    Ok(())
}

#[test]
fn add_unchanged_schema_fails() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;
    let s = make_schema(&[("a", "object")]);
    let result = repo.add(&s);
    assert!(result.is_err());
    Ok(())
}

// ===========================================================================
// Group 2: Linear History
// ===========================================================================

#[test]
fn linear_three_commits() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    let s1 = make_schema(&[("a", "object")]);
    repo.add(&s1)?;
    repo.commit("first", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    repo.commit("second", "alice")?;

    let s3 = make_schema(&[("a", "object"), ("b", "string"), ("c", "integer")]);
    repo.add(&s3)?;
    repo.commit("third", "alice")?;

    let log = repo.log(None)?;
    assert_eq!(log.len(), 3);
    assert_eq!(log[0].message, "third");
    assert_eq!(log[1].message, "second");
    assert_eq!(log[2].message, "first");
    Ok(())
}

#[test]
fn log_with_limit() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    let s1 = make_schema(&[("a", "object")]);
    repo.add(&s1)?;
    repo.commit("first", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    repo.commit("second", "alice")?;

    let s3 = make_schema(&[("a", "object"), ("b", "string"), ("c", "integer")]);
    repo.add(&s3)?;
    repo.commit("third", "alice")?;

    let log = repo.log(Some(2))?;
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].message, "third");
    assert_eq!(log[1].message, "second");
    Ok(())
}

#[test]
fn head_advances_each_commit() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    let s1 = make_schema(&[("a", "object")]);
    repo.add(&s1)?;
    let c1 = repo.commit("first", "alice")?;
    assert_eq!(store::resolve_head(repo.store())?, Some(c1));

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    let c2 = repo.commit("second", "alice")?;
    assert_eq!(store::resolve_head(repo.store())?, Some(c2));
    assert_ne!(c1, c2);
    Ok(())
}

#[test]
fn commit_preserves_schema() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    let s = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s)?;
    repo.commit("initial", "alice")?;

    let log = repo.log(None)?;
    let obj = repo.store().get(&log[0].schema_id)?;
    match obj {
        panproto_vcs::Object::Schema(stored) => {
            assert!(stored.vertices.contains_key("a"));
            assert!(stored.vertices.contains_key("b"));
            assert_eq!(stored.vertices.len(), 2);
        }
        _ => panic!("expected schema object"),
    }
    Ok(())
}

// ===========================================================================
// Group 3: Branching & Checkout
// ===========================================================================

#[test]
fn create_branch_and_list() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "feature", c1)?;
    let branches = refs::list_branches(repo.store())?;
    let names: Vec<&str> = branches.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"main"));
    assert!(names.contains(&"feature"));
    Ok(())
}

#[test]
fn checkout_switches_head() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "dev", c1)?;
    refs::checkout_branch(repo.store_mut(), "dev")?;
    assert_eq!(repo.store().get_head()?, HeadState::Branch("dev".into()));
    Ok(())
}

#[test]
fn delete_branch() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::delete_branch(repo.store_mut(), "feature")?;
    let branches = refs::list_branches(repo.store())?;
    assert!(!branches.iter().any(|(n, _)| n == "feature"));
    Ok(())
}

#[test]
fn checkout_nonexistent_fails() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let _repo = Repository::init(dir.path())?;
    let mut repo = Repository::open(dir.path())?;
    let result = refs::checkout_branch(repo.store_mut(), "nonexistent");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn create_duplicate_branch_fails() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "feature", c1)?;
    let result = refs::create_branch(repo.store_mut(), "feature", c1);
    assert!(matches!(result, Err(VcsError::BranchExists { .. })));
    Ok(())
}

#[test]
fn checkout_detached() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::checkout_detached(repo.store_mut(), c1)?;
    assert_eq!(repo.store().get_head()?, HeadState::Detached(c1));
    Ok(())
}

#[test]
fn force_delete_unmerged_branch() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Create feature branch and add a commit on it.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    repo.commit("feature work", "bob")?;

    // Switch back to main.
    refs::checkout_branch(repo.store_mut(), "main")?;

    // Safe delete should fail because feature is not merged.
    let result = refs::delete_branch(repo.store_mut(), "feature");
    assert!(matches!(result, Err(VcsError::BranchNotMerged { .. })));

    // Force delete should succeed.
    refs::force_delete_branch(repo.store_mut(), "feature")?;
    let branches = refs::list_branches(repo.store())?;
    assert!(!branches.iter().any(|(n, _)| n == "feature"));
    Ok(())
}

#[test]
fn rename_branch() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "old-name", c1)?;
    refs::rename_branch(repo.store_mut(), "old-name", "new-name")?;

    let branches = refs::list_branches(repo.store())?;
    let names: Vec<&str> = branches.iter().map(|(n, _)| n.as_str()).collect();
    assert!(!names.contains(&"old-name"));
    assert!(names.contains(&"new-name"));

    // The renamed branch should point at the same commit.
    let resolved = refs::resolve_ref(repo.store(), "new-name")?;
    assert_eq!(resolved, c1);
    Ok(())
}

// ===========================================================================
// Group 4: Tags
// ===========================================================================

#[test]
fn create_tag_and_list() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_tag(repo.store_mut(), "v1.0", c1)?;
    let tags = refs::list_tags(repo.store())?;
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].0, "v1.0");
    assert_eq!(tags[0].1, c1);
    Ok(())
}

#[test]
fn delete_tag() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_tag(repo.store_mut(), "v1.0", c1)?;
    refs::delete_tag(repo.store_mut(), "v1.0")?;
    let tags = refs::list_tags(repo.store())?;
    assert!(tags.is_empty());
    Ok(())
}

#[test]
fn resolve_tag() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_tag(repo.store_mut(), "v1.0", c1)?;
    let resolved = refs::resolve_ref(repo.store(), "v1.0")?;
    assert_eq!(resolved, c1);
    Ok(())
}

#[test]
fn create_annotated_tag() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let tag_id = refs::create_annotated_tag(repo.store_mut(), "v2.0", c1, "alice", "release 2.0")?;

    // The tag ref should point to the tag object, not the commit directly.
    let tags = refs::list_tags(repo.store())?;
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].0, "v2.0");
    assert_eq!(tags[0].1, tag_id);

    // resolve_ref should peel through the tag object to the commit.
    let resolved = refs::resolve_ref(repo.store(), "v2.0")?;
    assert_eq!(resolved, c1);

    // Verify the tag object content.
    let obj = repo.store().get(&tag_id)?;
    match obj {
        panproto_vcs::Object::Tag(tag) => {
            assert_eq!(tag.target, c1);
            assert_eq!(tag.tagger, "alice");
            assert_eq!(tag.message, "release 2.0");
        }
        _ => panic!("expected tag object"),
    }
    Ok(())
}

#[test]
fn force_overwrite_tag() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_tag(repo.store_mut(), "v1.0", c1)?;

    // Add second commit.
    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    let c2 = repo.commit("second", "alice")?;

    // Normal create should fail.
    let result = refs::create_tag(repo.store_mut(), "v1.0", c2);
    assert!(matches!(result, Err(VcsError::TagExists { .. })));

    // Force create should succeed and update.
    refs::create_tag_force(repo.store_mut(), "v1.0", c2)?;
    let resolved = refs::resolve_ref(repo.store(), "v1.0")?;
    assert_eq!(resolved, c2);
    Ok(())
}

// ===========================================================================
// Group 5: Fast-Forward Merge
// ===========================================================================

#[test]
fn merge_fast_forward() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    repo.commit("add b", "bob")?;

    refs::checkout_branch(repo.store_mut(), "main")?;
    let result = repo.merge("feature", "alice")?;
    assert!(result.conflicts.is_empty());
    assert!(result.merged_schema.vertices.contains_key("b"));
    Ok(())
}

#[test]
fn merge_fast_forward_multiple_commits() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    repo.commit("add b", "bob")?;

    let s3 = make_schema(&[("a", "object"), ("b", "string"), ("c", "integer")]);
    repo.add(&s3)?;
    repo.commit("add c", "bob")?;

    refs::checkout_branch(repo.store_mut(), "main")?;
    let result = repo.merge("feature", "alice")?;
    assert!(result.conflicts.is_empty());

    let log = repo.log(None)?;
    assert_eq!(log.len(), 3); // 3 commits after fast-forward
    assert!(result.merged_schema.vertices.contains_key("b"));
    assert!(result.merged_schema.vertices.contains_key("c"));
    Ok(())
}

// ===========================================================================
// Group 6: Clean Three-Way Merge
// ===========================================================================

#[test]
fn merge_three_way_clean() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Feature branch adds "b".
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    repo.commit("add b", "bob")?;

    // Main branch adds "c".
    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "object"), ("c", "integer")]);
    repo.add(&sm)?;
    repo.commit("add c", "alice")?;

    // Merge feature into main.
    let result = repo.merge("feature", "alice")?;
    assert!(result.conflicts.is_empty());
    assert!(result.merged_schema.vertices.contains_key("a"));
    assert!(result.merged_schema.vertices.contains_key("b"));
    assert!(result.merged_schema.vertices.contains_key("c"));
    Ok(())
}

#[test]
fn merge_identical_additions() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Both branches add vertex "b" with same kind.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    repo.commit("add b on feature", "bob")?;

    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sm)?;
    repo.commit("add b on main", "alice")?;

    // Should merge cleanly since both added the same vertex.
    let result = repo.merge("feature", "alice")?;
    assert!(result.conflicts.is_empty());
    assert!(result.merged_schema.vertices.contains_key("b"));
    Ok(())
}

#[test]
fn merge_auto_commits() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    repo.commit("add b", "bob")?;

    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "object"), ("c", "integer")]);
    repo.add(&sm)?;
    repo.commit("add c", "alice")?;

    repo.merge("feature", "alice")?;

    // Merge auto-commit should have created a merge commit.
    let log = repo.log(None)?;
    // HEAD commit should be a merge commit with 2 parents.
    assert_eq!(log[0].parents.len(), 2);
    Ok(())
}

// ===========================================================================
// Group 7: Merge Options
// ===========================================================================

#[test]
fn merge_no_commit_leaves_staged() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    repo.commit("add b", "bob")?;

    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "object"), ("c", "integer")]);
    repo.add(&sm)?;
    let main_head = repo.commit("add c", "alice")?;

    let opts = MergeOptions {
        no_commit: true,
        ..Default::default()
    };
    let result = repo.merge_with_options("feature", "alice", &opts)?;
    assert!(result.conflicts.is_empty());

    // HEAD should NOT have advanced.
    let current_head = store::resolve_head(repo.store())?.unwrap();
    assert_eq!(current_head, main_head);
    Ok(())
}

#[test]
fn merge_ff_only_fails_on_diverge() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    repo.commit("add b", "bob")?;

    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "object"), ("c", "integer")]);
    repo.add(&sm)?;
    repo.commit("add c", "alice")?;

    let opts = MergeOptions {
        ff_only: true,
        ..Default::default()
    };
    let result = repo.merge_with_options("feature", "alice", &opts);
    assert!(matches!(result, Err(VcsError::FastForwardOnly)));
    Ok(())
}

#[test]
fn merge_no_ff_creates_commit() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Create feature that is strictly ahead (fast-forwardable).
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    repo.commit("add b", "bob")?;

    refs::checkout_branch(repo.store_mut(), "main")?;

    let opts = MergeOptions {
        no_ff: true,
        ..Default::default()
    };
    let result = repo.merge_with_options("feature", "alice", &opts)?;
    assert!(result.conflicts.is_empty());

    // Should have a merge commit even though fast-forward was possible.
    let log = repo.log(None)?;
    assert_eq!(log[0].parents.len(), 2);
    Ok(())
}

#[test]
fn merge_squash() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    repo.commit("add b", "bob")?;

    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "object"), ("c", "integer")]);
    repo.add(&sm)?;
    let main_head = repo.commit("add c", "alice")?;

    let opts = MergeOptions {
        squash: true,
        ..Default::default()
    };
    let result = repo.merge_with_options("feature", "alice", &opts)?;
    assert!(result.conflicts.is_empty());

    // HEAD should NOT have advanced because squash doesn't auto-commit.
    let current_head = store::resolve_head(repo.store())?.unwrap();
    assert_eq!(current_head, main_head);
    Ok(())
}

// ===========================================================================
// Group 8: Vertex Merge Conflicts
// ===========================================================================

#[test]
fn conflict_both_modified_vertex() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Feature changes "a" to "string".
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "string")]);
    repo.add(&sf)?;
    repo.commit("change a to string", "bob")?;

    // Main changes "a" to "integer".
    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "integer")]);
    repo.add(&sm)?;
    repo.commit("change a to integer", "alice")?;

    let result = repo.merge("feature", "alice")?;
    assert!(!result.conflicts.is_empty());
    assert!(result.conflicts.iter().any(|c| matches!(c,
        MergeConflict::BothModifiedVertex { vertex_id, .. } if vertex_id == "a"
    )));
    Ok(())
}

#[test]
fn conflict_both_added_vertex_differently() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Feature adds "b" as string.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    repo.commit("add b as string", "bob")?;

    // Main adds "b" as integer.
    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "object"), ("b", "integer")]);
    repo.add(&sm)?;
    repo.commit("add b as integer", "alice")?;

    let result = repo.merge("feature", "alice")?;
    assert!(!result.conflicts.is_empty());
    assert!(result.conflicts.iter().any(|c| matches!(c,
        MergeConflict::BothAddedVertexDifferently { vertex_id, .. } if vertex_id == "b"
    )));
    Ok(())
}

#[test]
fn conflict_delete_modify_vertex_ours() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(
        dir.path(),
        &[("a", "object"), ("b", "string")],
        "init",
        "alice",
    )?;

    // Feature modifies "b" to integer.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "integer")]);
    repo.add(&sf)?;
    repo.commit("change b to integer", "bob")?;

    // Main deletes "b".
    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "object")]);
    repo.add(&sm)?;
    repo.commit("delete b", "alice")?;

    let result = repo.merge("feature", "alice")?;
    assert!(!result.conflicts.is_empty());
    assert!(result.conflicts.iter().any(|c| matches!(c,
        MergeConflict::DeleteModifyVertex { vertex_id, deleted_by: Side::Ours } if vertex_id == "b"
    )));
    Ok(())
}

#[test]
fn conflict_delete_modify_vertex_theirs() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(
        dir.path(),
        &[("a", "object"), ("b", "string")],
        "init",
        "alice",
    )?;

    // Feature deletes "b".
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object")]);
    repo.add(&sf)?;
    repo.commit("delete b", "bob")?;

    // Main modifies "b" to integer.
    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "object"), ("b", "integer")]);
    repo.add(&sm)?;
    repo.commit("change b to integer", "alice")?;

    let result = repo.merge("feature", "alice")?;
    assert!(!result.conflicts.is_empty());
    assert!(result.conflicts.iter().any(|c| matches!(c,
        MergeConflict::DeleteModifyVertex { vertex_id, deleted_by: Side::Theirs } if vertex_id == "b"
    )));
    Ok(())
}

// ===========================================================================
// Group 9: Edge & Constraint Conflicts
// ===========================================================================

#[test]
fn edge_removal_one_side_is_clean() -> Result<(), Box<dyn std::error::Error>> {
    // In the pushout semantics, removing an edge from one side while the
    // other side keeps it is a one-sided change: the edge is removed from
    // the merged result. This should be a clean merge, not a conflict.
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // Base: a, b with edge a->b.
    let s_base = make_schema_with_edges(&[("a", "object"), ("b", "string")], &[("a", "b", "prop")]);
    repo.add(&s_base)?;
    let c1 = repo.commit("init", "alice")?;

    // Feature: remove the edge but keep vertices.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string"), ("c", "integer")]);
    repo.add(&sf)?;
    repo.commit("remove edge, add c", "bob")?;

    // Main: keep the edge, add vertex d.
    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema_with_edges(
        &[("a", "object"), ("b", "string"), ("d", "boolean")],
        &[("a", "b", "prop")],
    );
    repo.add(&sm)?;
    repo.commit("add d, keep edge", "alice")?;

    let result = repo.merge("feature", "alice")?;
    // Edge removal by one side is accepted — clean merge.
    assert!(result.conflicts.is_empty());
    // Merged schema should have a, b, c, d but no edge.
    assert!(result.merged_schema.vertices.contains_key("c"));
    assert!(result.merged_schema.vertices.contains_key("d"));
    assert!(result.merged_schema.edges.is_empty());
    Ok(())
}

#[test]
fn conflict_both_modified_constraint() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // Base: vertex "a" with constraint maxLength=100.
    let s_base = make_schema_with_constraints(&[("a", "string")], &[("a", "maxLength", "100")]);
    repo.add(&s_base)?;
    let c1 = repo.commit("init", "alice")?;

    // Feature changes maxLength to 200.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema_with_constraints(&[("a", "string")], &[("a", "maxLength", "200")]);
    repo.add(&sf)?;
    repo.commit("change maxLength to 200", "bob")?;

    // Main changes maxLength to 300.
    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema_with_constraints(&[("a", "string")], &[("a", "maxLength", "300")]);
    repo.add(&sm)?;
    repo.commit("change maxLength to 300", "alice")?;

    let result = repo.merge("feature", "alice")?;
    assert!(!result.conflicts.is_empty());
    assert!(result.conflicts.iter().any(|c| matches!(c,
        MergeConflict::BothModifiedConstraint { vertex_id, sort, .. }
            if vertex_id == "a" && sort == "maxLength"
    )));
    Ok(())
}

#[test]
fn conflict_both_added_constraint_differently() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "string")], "init", "alice")?;

    // Feature adds format=email.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema_with_constraints(&[("a", "string")], &[("a", "format", "email")]);
    repo.add(&sf)?;
    repo.commit("add format email", "bob")?;

    // Main adds format=uri.
    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema_with_constraints(&[("a", "string")], &[("a", "format", "uri")]);
    repo.add(&sm)?;
    repo.commit("add format uri", "alice")?;

    let result = repo.merge("feature", "alice")?;
    assert!(!result.conflicts.is_empty());
    assert!(result.conflicts.iter().any(|c| matches!(c,
        MergeConflict::BothAddedConstraintDifferently { vertex_id, sort, .. }
            if vertex_id == "a" && sort == "format"
    )));
    Ok(())
}

#[test]
fn conflict_delete_modify_constraint() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    let s_base = make_schema_with_constraints(&[("a", "string")], &[("a", "maxLength", "100")]);
    repo.add(&s_base)?;
    let c1 = repo.commit("init", "alice")?;

    // Feature modifies the constraint.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema_with_constraints(&[("a", "string")], &[("a", "maxLength", "200")]);
    repo.add(&sf)?;
    repo.commit("change maxLength to 200", "bob")?;

    // Main removes the constraint.
    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "string")]);
    repo.add(&sm)?;
    repo.commit("remove constraint", "alice")?;

    let result = repo.merge("feature", "alice")?;
    assert!(!result.conflicts.is_empty());
    assert!(result.conflicts.iter().any(|c| matches!(c,
        MergeConflict::DeleteModifyConstraint { vertex_id, sort, .. }
            if vertex_id == "a" && sort == "maxLength"
    )));
    Ok(())
}

// ===========================================================================
// Group 10: Other Element Conflicts
// ===========================================================================

#[test]
fn conflict_both_modified_nsid() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // Base: vertex "a" with nsid "com.example.base".
    let mut s_base = make_schema(&[("a", "object")]);
    s_base
        .nsids
        .insert(Name::from("a"), Name::from("com.example.base"));
    repo.add(&s_base)?;
    let c1 = repo.commit("init", "alice")?;

    // Feature changes nsid to "com.example.feature".
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let mut sf = make_schema(&[("a", "object")]);
    sf.nsids
        .insert(Name::from("a"), Name::from("com.example.feature"));
    repo.add(&sf)?;
    repo.commit("change nsid to feature", "bob")?;

    // Main changes nsid to "com.example.main".
    refs::checkout_branch(repo.store_mut(), "main")?;
    let mut sm = make_schema(&[("a", "object")]);
    sm.nsids
        .insert(Name::from("a"), Name::from("com.example.main"));
    repo.add(&sm)?;
    repo.commit("change nsid to main", "alice")?;

    let result = repo.merge("feature", "alice")?;
    assert!(!result.conflicts.is_empty());
    assert!(result.conflicts.iter().any(|c| matches!(c,
        MergeConflict::BothModifiedNsid { vertex_id, .. } if vertex_id == "a"
    )));
    Ok(())
}

#[test]
fn conflict_both_modified_ordering() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // Base: vertices a, b with edge a->b at position 0.
    let mut s_base =
        make_schema_with_edges(&[("a", "object"), ("b", "string")], &[("a", "b", "prop")]);
    let edge = Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "prop".into(),
        name: None,
    };
    s_base.orderings.insert(edge.clone(), 0);
    repo.add(&s_base)?;
    let c1 = repo.commit("init", "alice")?;

    // Feature changes ordering to 5.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let mut sf = make_schema_with_edges(&[("a", "object"), ("b", "string")], &[("a", "b", "prop")]);
    sf.orderings.insert(edge.clone(), 5);
    repo.add(&sf)?;
    repo.commit("set ordering to 5", "bob")?;

    // Main changes ordering to 10.
    refs::checkout_branch(repo.store_mut(), "main")?;
    let mut sm = make_schema_with_edges(&[("a", "object"), ("b", "string")], &[("a", "b", "prop")]);
    sm.orderings.insert(edge, 10);
    repo.add(&sm)?;
    repo.commit("set ordering to 10", "alice")?;

    let result = repo.merge("feature", "alice")?;
    assert!(!result.conflicts.is_empty());
    assert!(
        result
            .conflicts
            .iter()
            .any(|c| matches!(c, MergeConflict::BothModifiedOrdering { .. }))
    );
    Ok(())
}

#[test]
fn conflict_multiple_simultaneous() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(
        dir.path(),
        &[("a", "object"), ("b", "string")],
        "init",
        "alice",
    )?;

    // Feature changes both "a" and "b".
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "integer"), ("b", "integer")]);
    repo.add(&sf)?;
    repo.commit("change both to integer", "bob")?;

    // Main changes both "a" and "b" differently.
    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "array"), ("b", "array")]);
    repo.add(&sm)?;
    repo.commit("change both to array", "alice")?;

    let result = repo.merge("feature", "alice")?;
    assert!(result.conflicts.len() >= 2);
    Ok(())
}

// ===========================================================================
// Group 11: Cherry-Pick
// ===========================================================================

#[test]
fn cherry_pick_applies_change() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Create feature branch and add a commit.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    let feature_commit = repo.commit("add b", "bob")?;

    // Switch back to main.
    refs::checkout_branch(repo.store_mut(), "main")?;

    // Cherry-pick the feature commit.
    let new_id = repo.cherry_pick(feature_commit, "alice")?;

    // Verify the cherry-picked commit has vertex b.
    let obj = repo.store().get(&new_id)?;
    match obj {
        panproto_vcs::Object::Commit(c) => {
            let schema = repo.store().get(&c.schema_id)?;
            match schema {
                panproto_vcs::Object::Schema(s) => {
                    assert!(s.vertices.contains_key("b"));
                    assert!(s.vertices.contains_key("a"));
                }
                _ => panic!("expected schema"),
            }
        }
        _ => panic!("expected commit"),
    }
    Ok(())
}

#[test]
fn cherry_pick_conflict_fails() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Feature changes "a" to "string".
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "string")]);
    repo.add(&sf)?;
    let feature_commit = repo.commit("change a to string", "bob")?;

    // Main changes "a" to "integer".
    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "integer")]);
    repo.add(&sm)?;
    repo.commit("change a to integer", "alice")?;

    // Cherry-pick should fail due to conflict.
    let result = repo.cherry_pick(feature_commit, "alice");
    assert!(matches!(result, Err(VcsError::MergeConflicts { .. })));
    Ok(())
}

#[test]
fn cherry_pick_preserves_branch() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    let feature_commit = repo.commit("add b", "bob")?;

    refs::checkout_branch(repo.store_mut(), "main")?;

    // Remember main head before cherry-pick.
    let _main_head_before = store::resolve_head(repo.store())?.unwrap();

    repo.cherry_pick(feature_commit, "alice")?;

    // HEAD should still be on main branch.
    assert_eq!(repo.store().get_head()?, HeadState::Branch("main".into()));
    Ok(())
}

#[test]
fn cherry_pick_no_commit() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    let feature_commit = repo.commit("add b", "bob")?;

    refs::checkout_branch(repo.store_mut(), "main")?;
    let main_head = store::resolve_head(repo.store())?.unwrap();

    let opts = panproto_vcs::cherry_pick::CherryPickOptions {
        no_commit: true,
        record_origin: false,
    };
    let _schema_id = panproto_vcs::cherry_pick::cherry_pick_with_options(
        repo.store_mut(),
        feature_commit,
        "alice",
        &opts,
    )?;

    // HEAD should NOT have advanced.
    let current_head = store::resolve_head(repo.store())?.unwrap();
    assert_eq!(current_head, main_head);
    Ok(())
}

// ===========================================================================
// Group 12: Rebase
// ===========================================================================

#[test]
fn rebase_diverged_branch() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Main adds "b".
    let sm = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sm)?;
    let main_tip = repo.commit("add b on main", "alice")?;

    // Create feature off c1 and add "c".
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("c", "integer")]);
    repo.add(&sf)?;
    repo.commit("add c on feature", "bob")?;

    // Rebase feature onto main.
    let new_tip = repo.rebase(main_tip, "bob")?;

    // Rebased commit should have both "b" and "c".
    let obj = repo.store().get(&new_tip)?;
    match obj {
        panproto_vcs::Object::Commit(c) => {
            let schema = repo.store().get(&c.schema_id)?;
            match schema {
                panproto_vcs::Object::Schema(s) => {
                    assert!(s.vertices.contains_key("a"));
                    assert!(s.vertices.contains_key("b"));
                    assert!(s.vertices.contains_key("c"));
                }
                _ => panic!("expected schema"),
            }
        }
        _ => panic!("expected commit"),
    }
    Ok(())
}

#[test]
fn rebase_multiple_commits() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Main adds "b".
    let sm = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sm)?;
    let main_tip = repo.commit("add b", "alice")?;

    // Feature off c1: two commits adding "c" then "d".
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let s_c = make_schema(&[("a", "object"), ("c", "integer")]);
    repo.add(&s_c)?;
    repo.commit("add c", "bob")?;

    let s_d = make_schema(&[("a", "object"), ("c", "integer"), ("d", "boolean")]);
    repo.add(&s_d)?;
    repo.commit("add d", "bob")?;

    let new_tip = repo.rebase(main_tip, "bob")?;

    let obj = repo.store().get(&new_tip)?;
    match obj {
        panproto_vcs::Object::Commit(c) => {
            let schema = repo.store().get(&c.schema_id)?;
            match schema {
                panproto_vcs::Object::Schema(s) => {
                    assert!(s.vertices.contains_key("a"));
                    assert!(s.vertices.contains_key("b"));
                    assert!(s.vertices.contains_key("c"));
                    assert!(s.vertices.contains_key("d"));
                }
                _ => panic!("expected schema"),
            }
        }
        _ => panic!("expected commit"),
    }
    Ok(())
}

#[test]
fn rebase_conflict_fails() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Main changes "a" to "string".
    let sm = make_schema(&[("a", "string")]);
    repo.add(&sm)?;
    let main_tip = repo.commit("change a to string", "alice")?;

    // Feature changes "a" to "integer".
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "integer")]);
    repo.add(&sf)?;
    repo.commit("change a to integer", "bob")?;

    let result = repo.rebase(main_tip, "bob");
    assert!(matches!(result, Err(VcsError::MergeConflicts { .. })));
    Ok(())
}

// ===========================================================================
// Group 13: Amend
// ===========================================================================

#[test]
fn amend_changes_message() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let new_id = repo.amend("amended message", "alice")?;
    let log = repo.log(None)?;
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].message, "amended message");

    let head = store::resolve_head(repo.store())?.unwrap();
    assert_eq!(head, new_id);
    Ok(())
}

#[test]
fn amend_changes_schema() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Stage a new schema.
    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;

    // Amend with new schema and message.
    repo.amend("amended with b", "alice")?;

    let log = repo.log(None)?;
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].message, "amended with b");

    let schema = repo.store().get(&log[0].schema_id)?;
    match schema {
        panproto_vcs::Object::Schema(s) => {
            assert!(s.vertices.contains_key("b"));
        }
        _ => panic!("expected schema"),
    }
    Ok(())
}

#[test]
fn amend_no_commits_fails() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;
    let result = repo.amend("nothing here", "alice");
    assert!(matches!(result, Err(VcsError::NothingToAmend)));
    Ok(())
}

// ===========================================================================
// Group 14: Reset
// ===========================================================================

#[test]
fn reset_soft() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    let _c2 = repo.commit("second", "alice")?;

    let outcome = repo.reset(c1, ResetMode::Soft, "alice")?;
    assert!(!outcome.should_clear_index);
    assert!(!outcome.should_write_working);
    assert_eq!(outcome.new_head, c1);

    let head = store::resolve_head(repo.store())?.unwrap();
    assert_eq!(head, c1);
    Ok(())
}

#[test]
fn reset_mixed() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    let _c2 = repo.commit("second", "alice")?;

    let outcome = repo.reset(c1, ResetMode::Mixed, "alice")?;
    assert!(outcome.should_clear_index);
    assert!(!outcome.should_write_working);
    assert_eq!(outcome.new_head, c1);
    Ok(())
}

#[test]
fn reset_hard() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    let _c2 = repo.commit("second", "alice")?;

    let outcome = repo.reset(c1, ResetMode::Hard, "alice")?;
    assert!(outcome.should_clear_index);
    assert!(outcome.should_write_working);
    assert_eq!(outcome.new_head, c1);
    Ok(())
}

#[test]
fn reset_records_reflog() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    let _c2 = repo.commit("second", "alice")?;

    repo.reset(c1, ResetMode::Soft, "alice")?;

    let reflog = repo.store().read_reflog("HEAD", None)?;
    assert!(reflog.iter().any(|e| e.message.contains("reset")));
    Ok(())
}

// ===========================================================================
// Group 15: Stash
// ===========================================================================

#[test]
fn stash_push_pop() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Stage a schema to get its ID for stashing.
    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    let s2_obj = panproto_vcs::Object::Schema(Box::new(s2));
    let s2_id = repo.store_mut().put(&s2_obj)?;

    panproto_vcs::stash::stash_push(repo.store_mut(), s2_id, "alice", Some("wip"))?;
    let popped = panproto_vcs::stash::stash_pop(repo.store_mut())?;
    assert_eq!(popped, s2_id);
    Ok(())
}

#[test]
fn stash_multiple_lifo() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s1 = make_schema(&[("a", "object"), ("b", "string")]);
    let s1_id = repo
        .store_mut()
        .put(&panproto_vcs::Object::Schema(Box::new(s1)))?;

    let s2 = make_schema(&[("a", "object"), ("c", "integer")]);
    let s2_id = repo
        .store_mut()
        .put(&panproto_vcs::Object::Schema(Box::new(s2)))?;

    panproto_vcs::stash::stash_push(repo.store_mut(), s1_id, "alice", Some("first"))?;
    panproto_vcs::stash::stash_push(repo.store_mut(), s2_id, "alice", Some("second"))?;

    // LIFO: most recent first.
    let popped = panproto_vcs::stash::stash_pop(repo.store_mut())?;
    assert_eq!(popped, s2_id);
    Ok(())
}

#[test]
fn stash_pop_empty_fails() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let result = panproto_vcs::stash::stash_pop(repo.store_mut());
    assert!(result.is_err());
    Ok(())
}

#[test]
fn stash_list() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s1 = make_schema(&[("a", "object"), ("b", "string")]);
    let s1_id = repo
        .store_mut()
        .put(&panproto_vcs::Object::Schema(Box::new(s1)))?;

    let s2 = make_schema(&[("a", "object"), ("c", "integer")]);
    let s2_id = repo
        .store_mut()
        .put(&panproto_vcs::Object::Schema(Box::new(s2)))?;

    panproto_vcs::stash::stash_push(repo.store_mut(), s1_id, "alice", Some("first"))?;
    panproto_vcs::stash::stash_push(repo.store_mut(), s2_id, "alice", Some("second"))?;

    let entries = panproto_vcs::stash::stash_list(repo.store())?;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].message, "second");
    assert_eq!(entries[1].message, "first");
    Ok(())
}

#[test]
fn stash_apply_preserves_entry() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s1 = make_schema(&[("a", "object"), ("b", "string")]);
    let s1_id = repo
        .store_mut()
        .put(&panproto_vcs::Object::Schema(Box::new(s1)))?;

    panproto_vcs::stash::stash_push(repo.store_mut(), s1_id, "alice", Some("stashed"))?;

    // Apply should return the schema but keep the stash.
    let applied = panproto_vcs::stash::stash_apply(repo.store(), 0)?;
    assert_eq!(applied, s1_id);

    // Stash should still exist.
    let entries = panproto_vcs::stash::stash_list(repo.store())?;
    assert_eq!(entries.len(), 1);
    Ok(())
}

#[test]
fn stash_clear() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s1 = make_schema(&[("a", "object"), ("b", "string")]);
    let s1_id = repo
        .store_mut()
        .put(&panproto_vcs::Object::Schema(Box::new(s1)))?;

    panproto_vcs::stash::stash_push(repo.store_mut(), s1_id, "alice", Some("stash1"))?;
    panproto_vcs::stash::stash_push(repo.store_mut(), s1_id, "alice", Some("stash2"))?;

    panproto_vcs::stash::stash_clear(repo.store_mut())?;

    // Stash should be empty now.
    let result = panproto_vcs::stash::stash_pop(repo.store_mut());
    assert!(result.is_err());
    Ok(())
}

// ===========================================================================
// Group 16: Blame
// ===========================================================================

#[test]
fn blame_vertex_finds_introducer() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    let c2 = repo.commit("add b", "bob")?;

    let entry = panproto_vcs::blame::blame_vertex(repo.store(), c2, "b")?;
    assert_eq!(entry.commit_id, c2);
    assert_eq!(entry.author, "bob");
    Ok(())
}

#[test]
fn blame_vertex_root() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let entry = panproto_vcs::blame::blame_vertex(repo.store(), c1, "a")?;
    assert_eq!(entry.commit_id, c1);
    assert_eq!(entry.author, "alice");
    Ok(())
}

#[test]
fn blame_nonexistent_fails() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let result = panproto_vcs::blame::blame_vertex(repo.store(), c1, "nonexistent");
    assert!(result.is_err());
    Ok(())
}

// ===========================================================================
// Group 17: Bisect
// ===========================================================================

#[test]
fn bisect_finds_breaking_commit() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // Build a linear chain of 6 commits.
    // The "breaking" commit is commit 3 (zero-indexed).
    let mut ids = Vec::new();

    let s0 = make_schema(&[("a", "object")]);
    repo.add(&s0)?;
    ids.push(repo.commit("commit 0", "alice")?);

    let s1 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s1)?;
    ids.push(repo.commit("commit 1", "alice")?);

    let s2 = make_schema(&[("a", "object"), ("b", "string"), ("c", "integer")]);
    repo.add(&s2)?;
    ids.push(repo.commit("commit 2", "alice")?);

    // Commit 3: the "breaking" change.
    let s3 = make_schema(&[
        ("a", "object"),
        ("b", "string"),
        ("c", "integer"),
        ("d", "broken"),
    ]);
    repo.add(&s3)?;
    ids.push(repo.commit("commit 3 (breaking)", "alice")?);

    let s4 = make_schema(&[
        ("a", "object"),
        ("b", "string"),
        ("c", "integer"),
        ("d", "broken"),
        ("e", "extra"),
    ]);
    repo.add(&s4)?;
    ids.push(repo.commit("commit 4", "alice")?);

    let s5 = make_schema(&[
        ("a", "object"),
        ("b", "string"),
        ("c", "integer"),
        ("d", "broken"),
        ("e", "extra"),
        ("f", "another"),
    ]);
    repo.add(&s5)?;
    ids.push(repo.commit("commit 5", "alice")?);

    let breaking_index = 3;
    let (mut state, step) = panproto_vcs::bisect::bisect_start(repo.store(), ids[0], ids[5])?;

    let mut current_step = step;
    let mut steps = 0;

    loop {
        match current_step {
            panproto_vcs::bisect::BisectStep::Found(id) => {
                assert_eq!(id, ids[breaking_index]);
                break;
            }
            panproto_vcs::bisect::BisectStep::Test(id) => {
                let idx = ids.iter().position(|i| *i == id).unwrap();
                let is_good = idx < breaking_index;
                current_step = panproto_vcs::bisect::bisect_step(&mut state, is_good);
                steps += 1;
                assert!(steps <= 10, "bisect should converge");
            }
        }
    }
    Ok(())
}

#[test]
fn bisect_adjacent_found_immediately() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    let s1 = make_schema(&[("a", "object")]);
    repo.add(&s1)?;
    let c1 = repo.commit("good", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    let c2 = repo.commit("bad", "alice")?;

    let (_state, step) = panproto_vcs::bisect::bisect_start(repo.store(), c1, c2)?;
    assert!(matches!(step, panproto_vcs::bisect::BisectStep::Found(id) if id == c2));
    Ok(())
}

// ===========================================================================
// Group 18: GC
// ===========================================================================

#[test]
fn gc_after_reset() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    let c2 = repo.commit("second", "alice")?;

    // Reset to c1.
    repo.reset(c1, ResetMode::Hard, "alice")?;

    // Before GC: c2's objects still exist.
    assert!(repo.store().has(&c2));

    // Run GC.
    let report = repo.gc()?;
    assert!(!report.deleted.is_empty());
    assert!(!repo.store().has(&c2));
    Ok(())
}

#[test]
fn gc_preserves_tagged() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    let c2 = repo.commit("second", "alice")?;

    // Tag c2.
    refs::create_tag(repo.store_mut(), "v1.0", c2)?;

    // Reset to c1.
    repo.reset(c1, ResetMode::Hard, "alice")?;

    // Run GC: c2 should be preserved because it's tagged.
    let report = repo.gc()?;
    assert!(repo.store().has(&c2));
    // The deleted list should not contain c2.
    assert!(!report.deleted.contains(&c2));
    Ok(())
}

#[test]
fn gc_preserves_branches() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Create feature with a commit.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    let feature_commit = repo.commit("feature work", "bob")?;

    // Switch to main and reset (doesn't affect feature).
    refs::checkout_branch(repo.store_mut(), "main")?;

    // Run GC.
    let report = repo.gc()?;

    // Feature commit should be preserved.
    assert!(repo.store().has(&feature_commit));
    assert!(!report.deleted.contains(&feature_commit));
    Ok(())
}

// ===========================================================================
// Group 19: Reflog
// ===========================================================================

#[test]
fn reflog_records_commits() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    repo.commit("second", "alice")?;

    let reflog = repo.store().read_reflog("HEAD", None)?;
    assert!(!reflog.is_empty());

    // At least one entry should contain "commit".
    assert!(reflog.iter().any(|e| e.message.contains("commit")));
    Ok(())
}

#[test]
fn reflog_records_merge() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    repo.commit("add b", "bob")?;

    refs::checkout_branch(repo.store_mut(), "main")?;
    repo.merge("feature", "alice")?;

    let reflog = repo.store().read_reflog("HEAD", None)?;
    assert!(reflog.iter().any(|e| e.message.contains("merge")));
    Ok(())
}

// ===========================================================================
// Group 20: Compound Workflows
// ===========================================================================

#[test]
fn feature_branch_full_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // 1. Create and checkout feature branch.
    refs::create_and_checkout_branch(repo.store_mut(), "feature", c1)?;
    assert_eq!(
        repo.store().get_head()?,
        HeadState::Branch("feature".into())
    );

    // 2. Make two commits on feature.
    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    repo.commit("add b", "bob")?;

    let s3 = make_schema(&[("a", "object"), ("b", "string"), ("c", "integer")]);
    repo.add(&s3)?;
    repo.commit("add c", "bob")?;

    // 3. Switch to main.
    refs::checkout_branch(repo.store_mut(), "main")?;

    // 4. Merge feature.
    let result = repo.merge("feature", "alice")?;
    assert!(result.conflicts.is_empty());

    // 5. Verify merged schema.
    let log = repo.log(None)?;
    let head_schema_id = log[0].schema_id;
    let schema = repo.store().get(&head_schema_id)?;
    match schema {
        panproto_vcs::Object::Schema(s) => {
            assert!(s.vertices.contains_key("a"));
            assert!(s.vertices.contains_key("b"));
            assert!(s.vertices.contains_key("c"));
        }
        _ => panic!("expected schema"),
    }

    // 6. Tag the release.
    let head = store::resolve_head(repo.store())?.unwrap();
    refs::create_tag(repo.store_mut(), "v1.0", head)?;

    // 7. Delete the feature branch.
    refs::delete_branch(repo.store_mut(), "feature")?;

    // 8. Verify final state.
    let branches = refs::list_branches(repo.store())?;
    let names: Vec<&str> = branches.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"main"));
    assert!(!names.contains(&"feature"));

    let tags = refs::list_tags(repo.store())?;
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].0, "v1.0");
    Ok(())
}

#[test]
fn stash_across_branch_switch() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Create feature branch.
    refs::create_branch(repo.store_mut(), "feature", c1)?;

    // On main, prepare a schema to stash.
    let s_wip = make_schema(&[("a", "object"), ("wip", "string")]);
    let wip_id = repo
        .store_mut()
        .put(&panproto_vcs::Object::Schema(Box::new(s_wip)))?;

    // Stash the WIP.
    panproto_vcs::stash::stash_push(repo.store_mut(), wip_id, "alice", Some("wip on main"))?;

    // Switch to feature.
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    repo.commit("feature work", "bob")?;

    // Switch back to main.
    refs::checkout_branch(repo.store_mut(), "main")?;

    // Pop the stash.
    let popped = panproto_vcs::stash::stash_pop(repo.store_mut())?;
    assert_eq!(popped, wip_id);

    // Verify stash ref is gone (another pop should fail).
    let result = panproto_vcs::stash::stash_pop(repo.store_mut());
    assert!(result.is_err());
    Ok(())
}

#[test]
fn rebase_then_fast_forward_merge() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Main advances.
    let sm = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sm)?;
    let main_tip = repo.commit("add b on main", "alice")?;

    // Feature off c1.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("c", "integer")]);
    repo.add(&sf)?;
    repo.commit("add c on feature", "bob")?;

    // Rebase feature onto main.
    let _rebased_tip = repo.rebase(main_tip, "bob")?;

    // Switch to main.
    refs::checkout_branch(repo.store_mut(), "main")?;

    // Now feature is ahead of main (linear), so merge should fast-forward.
    let result = repo.merge("feature", "alice")?;
    assert!(result.conflicts.is_empty());

    // Main should now have all vertices.
    let log = repo.log(None)?;
    let schema = repo.store().get(&log[0].schema_id)?;
    match schema {
        panproto_vcs::Object::Schema(s) => {
            assert!(s.vertices.contains_key("a"));
            assert!(s.vertices.contains_key("b"));
            assert!(s.vertices.contains_key("c"));
        }
        _ => panic!("expected schema"),
    }
    Ok(())
}

#[test]
fn reset_then_recommit_then_gc() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Second commit.
    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    let c2 = repo.commit("add b", "alice")?;

    // Third commit.
    let s3 = make_schema(&[("a", "object"), ("b", "string"), ("c", "integer")]);
    repo.add(&s3)?;
    let c3 = repo.commit("add c", "alice")?;

    // Reset to c1.
    repo.reset(c1, ResetMode::Hard, "alice")?;

    // Recommit with different content.
    let s_new = make_schema(&[("a", "object"), ("d", "boolean")]);
    repo.add(&s_new)?;
    let c_new = repo.commit("add d instead", "alice")?;

    // c2 and c3 should be unreachable now.
    let report = repo.gc()?;
    assert!(!report.deleted.is_empty());
    assert!(!repo.store().has(&c2));
    assert!(!repo.store().has(&c3));

    // The new commit should survive.
    assert!(repo.store().has(&c_new));

    // Verify the schema has "d" but not "b" or "c".
    let log = repo.log(None)?;
    assert_eq!(log.len(), 2);
    let schema = repo.store().get(&log[0].schema_id)?;
    match schema {
        panproto_vcs::Object::Schema(s) => {
            assert!(s.vertices.contains_key("a"));
            assert!(s.vertices.contains_key("d"));
            assert!(!s.vertices.contains_key("b"));
            assert!(!s.vertices.contains_key("c"));
        }
        _ => panic!("expected schema"),
    }
    Ok(())
}

// ===========================================================================
// Group 11: Coverage Gap Tests
// ===========================================================================

#[test]
fn blame_edge_finds_introducer() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _c1) = init_with_schema(
        dir.path(),
        &[("a", "object"), ("b", "string")],
        "init",
        "alice",
    )?;

    // Second commit adds an edge a->b.
    let s2 = make_schema_with_edges(&[("a", "object"), ("b", "string")], &[("a", "b", "prop")]);
    repo.add(&s2)?;
    let c2 = repo.commit("add edge", "alice")?;

    let edge = Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "prop".into(),
        name: None,
    };
    let entry = panproto_vcs::blame::blame_edge(repo.store(), c2, &edge)?;
    assert_eq!(entry.commit_id, c2);
    Ok(())
}

#[test]
fn blame_constraint_finds_introducer() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Second commit adds a constraint on vertex a.
    let s2 = make_schema_with_constraints(&[("a", "object")], &[("a", "maxLength", "100")]);
    repo.add(&s2)?;
    let c2 = repo.commit("add constraint", "alice")?;

    let entry = panproto_vcs::blame::blame_constraint(repo.store(), c2, "a", "maxLength")?;
    assert_eq!(entry.commit_id, c2);
    Ok(())
}

#[test]
fn dag_is_ancestor() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c0) = init_with_schema(dir.path(), &[("a", "object")], "c0", "alice")?;

    let s1 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s1)?;
    let c1 = repo.commit("c1", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string"), ("c", "integer")]);
    repo.add(&s2)?;
    let c2 = repo.commit("c2", "alice")?;

    // c0 is an ancestor of c2.
    assert!(panproto_vcs::dag::is_ancestor(repo.store(), c0, c2)?);
    // c2 is NOT an ancestor of c0.
    assert!(!panproto_vcs::dag::is_ancestor(repo.store(), c2, c0)?);
    // c0 is_ancestor with itself returns true (same-ID check).
    assert!(panproto_vcs::dag::is_ancestor(repo.store(), c0, c0)?);
    // c1 is an ancestor of c2.
    assert!(panproto_vcs::dag::is_ancestor(repo.store(), c1, c2)?);
    Ok(())
}

#[test]
fn dag_merge_base_diamond() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c0) = init_with_schema(dir.path(), &[("a", "object")], "base", "alice")?;

    // Main branch: add vertex b.
    let sm = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sm)?;
    let c1 = repo.commit("main work", "alice")?;

    // Feature branch from c0: add vertex c.
    refs::create_branch(repo.store_mut(), "feature", c0)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("c", "integer")]);
    repo.add(&sf)?;
    let c2 = repo.commit("feature work", "bob")?;

    let base = panproto_vcs::dag::merge_base(repo.store(), c1, c2)?;
    assert_eq!(base, Some(c0));
    Ok(())
}

#[test]
fn dag_commit_count_linear() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c0) = init_with_schema(dir.path(), &[("a", "object")], "c0", "alice")?;

    let s1 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s1)?;
    let _c1 = repo.commit("c1", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string"), ("c", "integer")]);
    repo.add(&s2)?;
    let _c2 = repo.commit("c2", "alice")?;

    let s3 = make_schema(&[
        ("a", "object"),
        ("b", "string"),
        ("c", "integer"),
        ("d", "boolean"),
    ]);
    repo.add(&s3)?;
    let c3 = repo.commit("c3", "alice")?;

    let count = panproto_vcs::dag::commit_count(repo.store(), c0, c3)?;
    assert_eq!(count, 3);
    Ok(())
}

#[test]
fn stash_drop_removes_entry() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, _c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s1 = make_schema(&[("a", "object"), ("b", "string")]);
    let s1_id = repo
        .store_mut()
        .put(&panproto_vcs::Object::Schema(Box::new(s1)))?;

    let s2 = make_schema(&[("a", "object"), ("c", "integer")]);
    let s2_id = repo
        .store_mut()
        .put(&panproto_vcs::Object::Schema(Box::new(s2)))?;

    panproto_vcs::stash::stash_push(repo.store_mut(), s1_id, "alice", Some("first"))?;
    panproto_vcs::stash::stash_push(repo.store_mut(), s2_id, "alice", Some("second"))?;

    // Drop index 0 (the most recent): this pops the top entry.
    panproto_vcs::stash::stash_drop(repo.store_mut(), 0)?;

    // After dropping, the stash ref should point to the first stash entry.
    // Verify by popping: should get s1_id back.
    let remaining = panproto_vcs::stash::stash_pop(repo.store_mut())?;
    assert_eq!(remaining, s1_id);

    // Dropping index 1 should fail (only index 0 is supported).
    // Re-push something so we can test the index check.
    panproto_vcs::stash::stash_push(repo.store_mut(), s2_id, "alice", Some("re-push"))?;
    let result = panproto_vcs::stash::stash_drop(repo.store_mut(), 1);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn create_and_checkout_branch_switches_head() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    refs::create_and_checkout_branch(repo.store_mut(), "feature", c1)?;

    // HEAD should now point to "feature".
    let head = repo.store().get_head()?;
    assert_eq!(head, HeadState::Branch("feature".into()));

    // The branch should appear in the list.
    let branches = refs::list_branches(repo.store())?;
    assert!(branches.iter().any(|(name, _)| name == "feature"));
    Ok(())
}

#[test]
fn annotated_tag_peels_on_resolve() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let tag_obj_id =
        refs::create_annotated_tag(repo.store_mut(), "v3.0", c1, "alice", "annotated release")?;

    // The tag ref points to the tag object.
    let tags = refs::list_tags(repo.store())?;
    let tag_entry = tags.iter().find(|(n, _)| n == "v3.0").unwrap();
    assert_eq!(tag_entry.1, tag_obj_id);
    assert_ne!(tag_obj_id, c1); // tag object != commit

    // resolve_ref should peel through to the commit.
    let resolved = refs::resolve_ref(repo.store(), "v3.0")?;
    assert_eq!(resolved, c1);
    Ok(())
}

#[test]
fn merge_with_custom_message() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Feature branch.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    repo.commit("add b", "bob")?;

    // Main branch diverges.
    refs::checkout_branch(repo.store_mut(), "main")?;
    let sm = make_schema(&[("a", "object"), ("c", "integer")]);
    repo.add(&sm)?;
    repo.commit("add c", "alice")?;

    let opts = MergeOptions {
        message: Some("custom msg".into()),
        ..Default::default()
    };
    repo.merge_with_options("feature", "alice", &opts)?;

    let log = repo.log(None)?;
    assert_eq!(log[0].message, "custom msg");
    assert_eq!(log[0].parents.len(), 2);
    Ok(())
}

#[test]
fn gc_dry_run_reports_but_preserves() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s2)?;
    let c2 = repo.commit("second", "alice")?;

    // Reset to c1, making c2 unreachable.
    repo.reset(c1, ResetMode::Hard, "alice")?;
    assert!(repo.store().has(&c2));

    // Dry-run GC: should report c2 as deletable but not actually delete.
    let options = panproto_vcs::gc::GcOptions { dry_run: true };
    let report = panproto_vcs::gc::gc_with_options(repo.store_mut(), &options)?;
    assert!(!report.deleted.is_empty());
    // Objects should still exist.
    assert!(repo.store().has(&c2));
    Ok(())
}

#[test]
fn reset_to_specific_commit_then_recommit() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c0) = init_with_schema(dir.path(), &[("a", "object")], "c0", "alice")?;

    let s1 = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&s1)?;
    let c1 = repo.commit("c1", "alice")?;

    let s2 = make_schema(&[("a", "object"), ("b", "string"), ("c", "integer")]);
    repo.add(&s2)?;
    let _c2 = repo.commit("c2", "alice")?;

    // Reset (mixed) to c1.
    repo.reset(c1, ResetMode::Mixed, "alice")?;

    // HEAD should be at c1.
    let head = store::resolve_head(repo.store())?.unwrap();
    assert_eq!(head, c1);

    // Add new schema and commit on top of c1.
    let s_new = make_schema(&[("a", "object"), ("b", "string"), ("d", "boolean")]);
    repo.add(&s_new)?;
    let c_new = repo.commit("add d", "alice")?;

    // Verify new commit's parent is c1.
    let log = repo.log(None)?;
    assert_eq!(log[0].message, "add d");
    assert_eq!(log[0].parents, vec![c1]);

    // c0 should still be ancestor.
    assert!(panproto_vcs::dag::is_ancestor(repo.store(), c0, c_new)?);
    Ok(())
}

#[test]
fn cherry_pick_record_origin() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let (mut repo, c1) = init_with_schema(dir.path(), &[("a", "object")], "init", "alice")?;

    // Feature branch.
    refs::create_branch(repo.store_mut(), "feature", c1)?;
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let sf = make_schema(&[("a", "object"), ("b", "string")]);
    repo.add(&sf)?;
    let feature_commit = repo.commit("add b on feature", "bob")?;

    // Back to main.
    refs::checkout_branch(repo.store_mut(), "main")?;

    let options = panproto_vcs::cherry_pick::CherryPickOptions {
        record_origin: true,
        no_commit: false,
    };
    let new_id = panproto_vcs::cherry_pick::cherry_pick_with_options(
        repo.store_mut(),
        feature_commit,
        "alice",
        &options,
    )?;

    // Load the new commit and check its message.
    let obj = repo.store().get(&new_id)?;
    match obj {
        panproto_vcs::Object::Commit(c) => {
            assert!(
                c.message.contains("(cherry picked from commit"),
                "expected origin annotation, got: {}",
                c.message
            );
        }
        _ => panic!("expected commit"),
    }
    Ok(())
}

// ===========================================================================
// Group 13: DAG Migration Composition (data lifting through schema migrations)
// ===========================================================================

/// Two-step DAG `compose_path`: c0 (name) -> c1 (name, email) -> c2 (name, email, role).
///
/// Composes the two auto-derived migrations into a single c0->c2 migration
/// and verifies the composed `vertex_map` maps the surviving c0 vertices to
/// their c2 counterparts.
#[test]
fn dag_compose_path_two_steps() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // c0: root with field "name"
    let s0 = make_schema_with_named_edges(
        &[("root", "object"), ("root.name", "string")],
        &[("root", "root.name", "prop", "name")],
    );
    repo.add(&s0)?;
    let c0 = repo.commit("v0: name only", "alice")?;

    // c1: root with fields "name" and "email"
    let s1 = make_schema_with_named_edges(
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.email", "string"),
        ],
        &[
            ("root", "root.name", "prop", "name"),
            ("root", "root.email", "prop", "email"),
        ],
    );
    repo.add(&s1)?;
    let c1 = repo.commit("v1: add email", "alice")?;

    // c2: root with fields "name", "email", and "role"
    let s2 = make_schema_with_named_edges(
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.email", "string"),
            ("root.role", "string"),
        ],
        &[
            ("root", "root.name", "prop", "name"),
            ("root", "root.email", "prop", "email"),
            ("root", "root.role", "prop", "role"),
        ],
    );
    repo.add(&s2)?;
    let c2 = repo.commit("v2: add role", "alice")?;

    // Compose c0 -> c1 -> c2 into a single migration.
    let path = vec![c0, c1, c2];
    let composed = dag::compose_path(repo.store(), &path)?;

    // The composed migration should map surviving c0 vertices (root, root.name)
    // to their c2 counterparts (identity, since names didn't change).
    assert_eq!(
        composed.vertex_map.get("root"),
        Some(&Name::from("root")),
        "root vertex should survive composition"
    );
    assert_eq!(
        composed.vertex_map.get("root.name"),
        Some(&Name::from("root.name")),
        "root.name vertex should survive composition"
    );
    // c0 didn't have root.email or root.role, so they shouldn't appear in
    // the composed migration's domain.
    assert!(
        !composed.vertex_map.contains_key("root.email"),
        "root.email was not in c0, should not be in composed domain"
    );
    assert!(
        !composed.vertex_map.contains_key("root.role"),
        "root.role was not in c0, should not be in composed domain"
    );

    Ok(())
}

/// Three-step DAG `compose_path`: c0 -> c1 -> c2 -> c3 with evolving schemas.
///
/// c0: (root, name)
/// c1: (root, name, email)
/// c2: (root, name, email, role)
/// c3: (root, name, role) — drops email
///
/// After composing c0->c3, the `vertex_map` should have root + name (email was
/// added and then dropped; name survives throughout; role is new in c2).
#[test]
fn dag_compose_path_three_steps() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    let s0 = make_schema_with_named_edges(
        &[("root", "object"), ("root.name", "string")],
        &[("root", "root.name", "prop", "name")],
    );
    repo.add(&s0)?;
    let c0 = repo.commit("v0", "alice")?;

    let s1 = make_schema_with_named_edges(
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.email", "string"),
        ],
        &[
            ("root", "root.name", "prop", "name"),
            ("root", "root.email", "prop", "email"),
        ],
    );
    repo.add(&s1)?;
    let c1 = repo.commit("v1: add email", "alice")?;

    let s2 = make_schema_with_named_edges(
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.email", "string"),
            ("root.role", "string"),
        ],
        &[
            ("root", "root.name", "prop", "name"),
            ("root", "root.email", "prop", "email"),
            ("root", "root.role", "prop", "role"),
        ],
    );
    repo.add(&s2)?;
    let c2 = repo.commit("v2: add role", "alice")?;

    // c3 drops email, keeps name and role.
    let s3 = make_schema_with_named_edges(
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.role", "string"),
        ],
        &[
            ("root", "root.name", "prop", "name"),
            ("root", "root.role", "prop", "role"),
        ],
    );
    repo.add(&s3)?;
    let c3 = repo.commit("v3: drop email", "alice")?;

    let path = vec![c0, c1, c2, c3];
    let composed = dag::compose_path(repo.store(), &path)?;

    // root and root.name survive all three steps.
    assert_eq!(composed.vertex_map.get("root"), Some(&Name::from("root")));
    assert_eq!(
        composed.vertex_map.get("root.name"),
        Some(&Name::from("root.name"))
    );
    // root.email was added in c1 then dropped in c3 — since it was never in c0,
    // it shouldn't appear in the composed migration's domain at all.
    assert!(!composed.vertex_map.contains_key("root.email"));
    // root.role was not in c0, so it shouldn't appear in the domain.
    assert!(!composed.vertex_map.contains_key("root.role"));

    Ok(())
}

/// Compose path using `dag::find_path` to discover the route, then compose.
#[test]
fn dag_find_path_then_compose() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    let s0 = make_schema_with_named_edges(
        &[("root", "object"), ("root.x", "string")],
        &[("root", "root.x", "prop", "x")],
    );
    repo.add(&s0)?;
    let c0 = repo.commit("v0", "alice")?;

    let s1 = make_schema_with_named_edges(
        &[
            ("root", "object"),
            ("root.x", "string"),
            ("root.y", "string"),
        ],
        &[
            ("root", "root.x", "prop", "x"),
            ("root", "root.y", "prop", "y"),
        ],
    );
    repo.add(&s1)?;
    let _c1 = repo.commit("v1: add y", "alice")?;

    let s2 = make_schema_with_named_edges(
        &[
            ("root", "object"),
            ("root.x", "string"),
            ("root.y", "string"),
            ("root.z", "integer"),
        ],
        &[
            ("root", "root.x", "prop", "x"),
            ("root", "root.y", "prop", "y"),
            ("root", "root.z", "prop", "z"),
        ],
    );
    repo.add(&s2)?;
    let c2 = repo.commit("v2: add z", "alice")?;

    // Use find_path to discover the route.
    let path = dag::find_path(repo.store(), c0, c2)?;
    assert_eq!(path.len(), 3, "path should have 3 commits");
    assert_eq!(path[0], c0);
    assert_eq!(path[2], c2);

    let composed = dag::compose_path(repo.store(), &path)?;
    assert_eq!(composed.vertex_map.get("root"), Some(&Name::from("root")));
    assert_eq!(
        composed.vertex_map.get("root.x"),
        Some(&Name::from("root.x"))
    );
    // y and z are new, not in c0's domain.
    assert!(!composed.vertex_map.contains_key("root.y"));
    assert!(!composed.vertex_map.contains_key("root.z"));

    // The edge map should include the surviving edge (root -> root.x).
    let src_edge = Edge {
        src: "root".into(),
        tgt: "root.x".into(),
        kind: "prop".into(),
        name: Some("x".into()),
    };
    assert!(
        composed.edge_map.contains_key(&src_edge),
        "prop edge root->root.x should survive in composed migration"
    );

    Ok(())
}

/// Verify `compose_path` with a single-step path returns the step's migration directly.
#[test]
fn dag_compose_path_single_step() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    let s0 = make_schema_with_named_edges(
        &[("root", "object"), ("root.a", "string")],
        &[("root", "root.a", "prop", "a")],
    );
    repo.add(&s0)?;
    let c0 = repo.commit("v0", "alice")?;

    let s1 = make_schema_with_named_edges(
        &[
            ("root", "object"),
            ("root.a", "string"),
            ("root.b", "string"),
        ],
        &[
            ("root", "root.a", "prop", "a"),
            ("root", "root.b", "prop", "b"),
        ],
    );
    repo.add(&s1)?;
    let c1 = repo.commit("v1: add b", "alice")?;

    let path = vec![c0, c1];
    let composed = dag::compose_path(repo.store(), &path)?;

    // Single step: composed should be identical to the c0->c1 migration.
    assert_eq!(composed.vertex_map.get("root"), Some(&Name::from("root")));
    assert_eq!(
        composed.vertex_map.get("root.a"),
        Some(&Name::from("root.a"))
    );
    assert!(
        !composed.vertex_map.contains_key("root.b"),
        "root.b is new in c1, should not be in migration domain"
    );

    Ok(())
}
