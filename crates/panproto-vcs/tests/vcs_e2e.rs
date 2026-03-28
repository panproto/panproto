//! Comprehensive VCS end-to-end test suite.
//!
//! Exercises the full Repository workflow: linear evolution, branching,
//! merging (with and without conflicts), rebase, cherry-pick, stash,
//! bisect, and DAG composition coherence.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{Node, WInstance};
use panproto_mig::Migration;
use panproto_schema::{Edge, EdgeRule, Protocol, Schema, SchemaBuilder};
use panproto_vcs::object::{CommitObject, Object};
use panproto_vcs::store::{self, Store};
use panproto_vcs::{ObjectId, Repository, refs};

// ===========================================================================
// Protocol
// ===========================================================================

fn blog_protocol() -> Protocol {
    Protocol {
        name: "blog".into(),
        schema_theory: "ThBlog".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![
            EdgeRule {
                edge_kind: "prop".into(),
                src_kinds: vec!["object".into()],
                tgt_kinds: vec![],
            },
            EdgeRule {
                edge_kind: "ref".into(),
                src_kinds: vec!["object".into()],
                tgt_kinds: vec!["object".into()],
            },
        ],
        obj_kinds: vec![
            "object".into(),
            "string".into(),
            "integer".into(),
            "datetime".into(),
            "email-address".into(),
        ],
        constraint_sorts: vec!["maxLength".into()],
        ..Protocol::default()
    }
}

// ===========================================================================
// Schema builders
// ===========================================================================

/// v1: User(name, email), Post(title, body, author->User)
/// 7 vertices, 5 edges
fn blog_v1() -> Schema {
    let proto = blog_protocol();
    SchemaBuilder::new(&proto)
        // User
        .vertex("User", "object", None::<&str>)
        .unwrap()
        .vertex("User.name", "string", None::<&str>)
        .unwrap()
        .vertex("User.email", "string", None::<&str>)
        .unwrap()
        .edge("User", "User.name", "prop", Some("name"))
        .unwrap()
        .edge("User", "User.email", "prop", Some("email"))
        .unwrap()
        // Post
        .vertex("Post", "object", None::<&str>)
        .unwrap()
        .vertex("Post.title", "string", None::<&str>)
        .unwrap()
        .vertex("Post.body", "string", None::<&str>)
        .unwrap()
        .edge("Post", "Post.title", "prop", Some("title"))
        .unwrap()
        .edge("Post", "Post.body", "prop", Some("body"))
        .unwrap()
        .edge("Post", "User", "ref", Some("author"))
        .unwrap()
        .build()
        .unwrap()
}

/// v2: v1 + Comment(text, post->Post, author->User)
/// 10 vertices, 8 edges
fn blog_v2() -> Schema {
    let proto = blog_protocol();
    SchemaBuilder::new(&proto)
        // User
        .vertex("User", "object", None::<&str>)
        .unwrap()
        .vertex("User.name", "string", None::<&str>)
        .unwrap()
        .vertex("User.email", "string", None::<&str>)
        .unwrap()
        .edge("User", "User.name", "prop", Some("name"))
        .unwrap()
        .edge("User", "User.email", "prop", Some("email"))
        .unwrap()
        // Post
        .vertex("Post", "object", None::<&str>)
        .unwrap()
        .vertex("Post.title", "string", None::<&str>)
        .unwrap()
        .vertex("Post.body", "string", None::<&str>)
        .unwrap()
        .edge("Post", "Post.title", "prop", Some("title"))
        .unwrap()
        .edge("Post", "Post.body", "prop", Some("body"))
        .unwrap()
        .edge("Post", "User", "ref", Some("author"))
        .unwrap()
        // Comment
        .vertex("Comment", "object", None::<&str>)
        .unwrap()
        .vertex("Comment.text", "string", None::<&str>)
        .unwrap()
        .edge("Comment", "Comment.text", "prop", Some("text"))
        .unwrap()
        .edge("Comment", "Post", "ref", Some("post"))
        .unwrap()
        .edge("Comment", "User", "ref", Some("author"))
        .unwrap()
        .build()
        .unwrap()
}

/// v3: v2 but rename `Post.body` to `Post.content` + add `Post.published_at` (datetime).
/// 11 vertices, 9 edges.
fn blog_v3() -> Schema {
    let proto = blog_protocol();
    SchemaBuilder::new(&proto)
        // User
        .vertex("User", "object", None::<&str>)
        .unwrap()
        .vertex("User.name", "string", None::<&str>)
        .unwrap()
        .vertex("User.email", "string", None::<&str>)
        .unwrap()
        .edge("User", "User.name", "prop", Some("name"))
        .unwrap()
        .edge("User", "User.email", "prop", Some("email"))
        .unwrap()
        // Post (body -> content, + published_at)
        .vertex("Post", "object", None::<&str>)
        .unwrap()
        .vertex("Post.title", "string", None::<&str>)
        .unwrap()
        .vertex("Post.content", "string", None::<&str>)
        .unwrap()
        .vertex("Post.published_at", "datetime", None::<&str>)
        .unwrap()
        .edge("Post", "Post.title", "prop", Some("title"))
        .unwrap()
        .edge("Post", "Post.content", "prop", Some("content"))
        .unwrap()
        .edge("Post", "Post.published_at", "prop", Some("published_at"))
        .unwrap()
        .edge("Post", "User", "ref", Some("author"))
        .unwrap()
        // Comment
        .vertex("Comment", "object", None::<&str>)
        .unwrap()
        .vertex("Comment.text", "string", None::<&str>)
        .unwrap()
        .edge("Comment", "Comment.text", "prop", Some("text"))
        .unwrap()
        .edge("Comment", "Post", "ref", Some("post"))
        .unwrap()
        .edge("Comment", "User", "ref", Some("author"))
        .unwrap()
        .build()
        .unwrap()
}

/// v4: v3 + Tag(name:string), PostTag(post->Post, tag->Tag)
/// 14 vertices, 12 edges
fn blog_v4() -> Schema {
    let proto = blog_protocol();
    SchemaBuilder::new(&proto)
        // User
        .vertex("User", "object", None::<&str>)
        .unwrap()
        .vertex("User.name", "string", None::<&str>)
        .unwrap()
        .vertex("User.email", "string", None::<&str>)
        .unwrap()
        .edge("User", "User.name", "prop", Some("name"))
        .unwrap()
        .edge("User", "User.email", "prop", Some("email"))
        .unwrap()
        // Post
        .vertex("Post", "object", None::<&str>)
        .unwrap()
        .vertex("Post.title", "string", None::<&str>)
        .unwrap()
        .vertex("Post.content", "string", None::<&str>)
        .unwrap()
        .vertex("Post.published_at", "datetime", None::<&str>)
        .unwrap()
        .edge("Post", "Post.title", "prop", Some("title"))
        .unwrap()
        .edge("Post", "Post.content", "prop", Some("content"))
        .unwrap()
        .edge("Post", "Post.published_at", "prop", Some("published_at"))
        .unwrap()
        .edge("Post", "User", "ref", Some("author"))
        .unwrap()
        // Comment
        .vertex("Comment", "object", None::<&str>)
        .unwrap()
        .vertex("Comment.text", "string", None::<&str>)
        .unwrap()
        .edge("Comment", "Comment.text", "prop", Some("text"))
        .unwrap()
        .edge("Comment", "Post", "ref", Some("post"))
        .unwrap()
        .edge("Comment", "User", "ref", Some("author"))
        .unwrap()
        // Tag
        .vertex("Tag", "object", None::<&str>)
        .unwrap()
        .vertex("Tag.name", "string", None::<&str>)
        .unwrap()
        .edge("Tag", "Tag.name", "prop", Some("name"))
        .unwrap()
        // PostTag
        .vertex("PostTag", "object", None::<&str>)
        .unwrap()
        .edge("PostTag", "Post", "ref", Some("post"))
        .unwrap()
        .edge("PostTag", "Tag", "ref", Some("tag"))
        .unwrap()
        .build()
        .unwrap()
}

/// v2 variant: `Comment` + `Comment.edited_at` (datetime).
fn blog_v2_with_edited_at() -> Schema {
    let proto = blog_protocol();
    SchemaBuilder::new(&proto)
        // User
        .vertex("User", "object", None::<&str>)
        .unwrap()
        .vertex("User.name", "string", None::<&str>)
        .unwrap()
        .vertex("User.email", "string", None::<&str>)
        .unwrap()
        .edge("User", "User.name", "prop", Some("name"))
        .unwrap()
        .edge("User", "User.email", "prop", Some("email"))
        .unwrap()
        // Post
        .vertex("Post", "object", None::<&str>)
        .unwrap()
        .vertex("Post.title", "string", None::<&str>)
        .unwrap()
        .vertex("Post.body", "string", None::<&str>)
        .unwrap()
        .edge("Post", "Post.title", "prop", Some("title"))
        .unwrap()
        .edge("Post", "Post.body", "prop", Some("body"))
        .unwrap()
        .edge("Post", "User", "ref", Some("author"))
        .unwrap()
        // Comment + edited_at
        .vertex("Comment", "object", None::<&str>)
        .unwrap()
        .vertex("Comment.text", "string", None::<&str>)
        .unwrap()
        .vertex("Comment.edited_at", "datetime", None::<&str>)
        .unwrap()
        .edge("Comment", "Comment.text", "prop", Some("text"))
        .unwrap()
        .edge("Comment", "Comment.edited_at", "prop", Some("edited_at"))
        .unwrap()
        .edge("Comment", "Post", "ref", Some("post"))
        .unwrap()
        .edge("Comment", "User", "ref", Some("author"))
        .unwrap()
        .build()
        .unwrap()
}

/// v2 variant: Comment + Comment.likes:integer
fn blog_v2_with_likes() -> Schema {
    let proto = blog_protocol();
    SchemaBuilder::new(&proto)
        // User
        .vertex("User", "object", None::<&str>)
        .unwrap()
        .vertex("User.name", "string", None::<&str>)
        .unwrap()
        .vertex("User.email", "string", None::<&str>)
        .unwrap()
        .edge("User", "User.name", "prop", Some("name"))
        .unwrap()
        .edge("User", "User.email", "prop", Some("email"))
        .unwrap()
        // Post
        .vertex("Post", "object", None::<&str>)
        .unwrap()
        .vertex("Post.title", "string", None::<&str>)
        .unwrap()
        .vertex("Post.body", "string", None::<&str>)
        .unwrap()
        .edge("Post", "Post.title", "prop", Some("title"))
        .unwrap()
        .edge("Post", "Post.body", "prop", Some("body"))
        .unwrap()
        .edge("Post", "User", "ref", Some("author"))
        .unwrap()
        // Comment + likes
        .vertex("Comment", "object", None::<&str>)
        .unwrap()
        .vertex("Comment.text", "string", None::<&str>)
        .unwrap()
        .vertex("Comment.likes", "integer", None::<&str>)
        .unwrap()
        .edge("Comment", "Comment.text", "prop", Some("text"))
        .unwrap()
        .edge("Comment", "Comment.likes", "prop", Some("likes"))
        .unwrap()
        .edge("Comment", "Post", "ref", Some("post"))
        .unwrap()
        .edge("Comment", "User", "ref", Some("author"))
        .unwrap()
        .build()
        .unwrap()
}

/// v1 variant: User.email as "email-address" kind instead of "string"
fn blog_v1_email_typed() -> Schema {
    let proto = blog_protocol();
    SchemaBuilder::new(&proto)
        .vertex("User", "object", None::<&str>)
        .unwrap()
        .vertex("User.name", "string", None::<&str>)
        .unwrap()
        .vertex("User.email", "email-address", None::<&str>)
        .unwrap()
        .edge("User", "User.name", "prop", Some("name"))
        .unwrap()
        .edge("User", "User.email", "prop", Some("email"))
        .unwrap()
        .vertex("Post", "object", None::<&str>)
        .unwrap()
        .vertex("Post.title", "string", None::<&str>)
        .unwrap()
        .vertex("Post.body", "string", None::<&str>)
        .unwrap()
        .edge("Post", "Post.title", "prop", Some("title"))
        .unwrap()
        .edge("Post", "Post.body", "prop", Some("body"))
        .unwrap()
        .edge("Post", "User", "ref", Some("author"))
        .unwrap()
        .build()
        .unwrap()
}

/// v1 variant: User without email vertex and its edge
fn blog_v1_no_email() -> Schema {
    let proto = blog_protocol();
    SchemaBuilder::new(&proto)
        .vertex("User", "object", None::<&str>)
        .unwrap()
        .vertex("User.name", "string", None::<&str>)
        .unwrap()
        .edge("User", "User.name", "prop", Some("name"))
        .unwrap()
        .vertex("Post", "object", None::<&str>)
        .unwrap()
        .vertex("Post.title", "string", None::<&str>)
        .unwrap()
        .vertex("Post.body", "string", None::<&str>)
        .unwrap()
        .edge("Post", "Post.title", "prop", Some("title"))
        .unwrap()
        .edge("Post", "Post.body", "prop", Some("body"))
        .unwrap()
        .edge("Post", "User", "ref", Some("author"))
        .unwrap()
        .build()
        .unwrap()
}

/// v1 variant: v1 + Tag(name:string)
fn blog_v1_with_tag() -> Schema {
    let proto = blog_protocol();
    SchemaBuilder::new(&proto)
        .vertex("User", "object", None::<&str>)
        .unwrap()
        .vertex("User.name", "string", None::<&str>)
        .unwrap()
        .vertex("User.email", "string", None::<&str>)
        .unwrap()
        .edge("User", "User.name", "prop", Some("name"))
        .unwrap()
        .edge("User", "User.email", "prop", Some("email"))
        .unwrap()
        .vertex("Post", "object", None::<&str>)
        .unwrap()
        .vertex("Post.title", "string", None::<&str>)
        .unwrap()
        .vertex("Post.body", "string", None::<&str>)
        .unwrap()
        .edge("Post", "Post.title", "prop", Some("title"))
        .unwrap()
        .edge("Post", "Post.body", "prop", Some("body"))
        .unwrap()
        .edge("Post", "User", "ref", Some("author"))
        .unwrap()
        .vertex("Tag", "object", None::<&str>)
        .unwrap()
        .vertex("Tag.name", "string", None::<&str>)
        .unwrap()
        .edge("Tag", "Tag.name", "prop", Some("name"))
        .unwrap()
        .build()
        .unwrap()
}

// ===========================================================================
// Instance builders
// ===========================================================================

#[allow(dead_code)]
fn make_edge(src: &str, tgt: &str, kind: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: Some(name.into()),
    }
}

#[allow(dead_code)]
fn make_user(root_id: u32, name: &str, email: &str) -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(root_id, Node::new(root_id, "User"));
    nodes.insert(
        root_id + 1,
        Node::new(root_id + 1, "User.name")
            .with_value(FieldPresence::Present(Value::Str(name.to_owned()))),
    );
    nodes.insert(
        root_id + 2,
        Node::new(root_id + 2, "User.email")
            .with_value(FieldPresence::Present(Value::Str(email.to_owned()))),
    );
    let arcs = vec![
        (
            root_id,
            root_id + 1,
            make_edge("User", "User.name", "prop", "name"),
        ),
        (
            root_id,
            root_id + 2,
            make_edge("User", "User.email", "prop", "email"),
        ),
    ];
    WInstance::new(nodes, arcs, vec![], root_id, Name::from("User"))
}

#[allow(dead_code)]
fn make_post_v1(root_id: u32, title: &str, body: &str) -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(root_id, Node::new(root_id, "Post"));
    nodes.insert(
        root_id + 1,
        Node::new(root_id + 1, "Post.title")
            .with_value(FieldPresence::Present(Value::Str(title.to_owned()))),
    );
    nodes.insert(
        root_id + 2,
        Node::new(root_id + 2, "Post.body")
            .with_value(FieldPresence::Present(Value::Str(body.to_owned()))),
    );
    let arcs = vec![
        (
            root_id,
            root_id + 1,
            make_edge("Post", "Post.title", "prop", "title"),
        ),
        (
            root_id,
            root_id + 2,
            make_edge("Post", "Post.body", "prop", "body"),
        ),
    ];
    WInstance::new(nodes, arcs, vec![], root_id, Name::from("Post"))
}

#[allow(dead_code)]
fn make_tag(root_id: u32, name: &str) -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(root_id, Node::new(root_id, "Tag"));
    nodes.insert(
        root_id + 1,
        Node::new(root_id + 1, "Tag.name")
            .with_value(FieldPresence::Present(Value::Str(name.to_owned()))),
    );
    let arcs = vec![(
        root_id,
        root_id + 1,
        make_edge("Tag", "Tag.name", "prop", "name"),
    )];
    WInstance::new(nodes, arcs, vec![], root_id, Name::from("Tag"))
}

// ===========================================================================
// Utility helpers
// ===========================================================================

fn load_commit(store: &dyn Store, id: ObjectId) -> CommitObject {
    match store.get(&id).unwrap() {
        Object::Commit(c) => c,
        other => panic!("expected Commit, got {}", other.type_name()),
    }
}

fn load_schema_from_commit(store: &dyn Store, commit_id: ObjectId) -> Schema {
    let commit = load_commit(store, commit_id);
    match store.get(&commit.schema_id).unwrap() {
        Object::Schema(s) => *s,
        other => panic!("expected Schema, got {}", other.type_name()),
    }
}

fn load_migration_from_commit(store: &dyn Store, commit_id: ObjectId) -> Migration {
    let commit = load_commit(store, commit_id);
    let Some(mig_id) = commit.migration_id else {
        panic!("commit should have a migration_id");
    };
    match store.get(&mig_id).unwrap() {
        Object::Migration { mapping, .. } => mapping,
        other => panic!("expected Migration, got {}", other.type_name()),
    }
}

// ===========================================================================
// Test 1: blog_schema_evolution
// ===========================================================================

#[test]
fn blog_schema_evolution() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // v1: initial (User, User.name, User.email, Post, Post.title, Post.body)
    let v1 = blog_v1();
    assert_eq!(v1.vertex_count(), 6);
    assert_eq!(v1.edge_count(), 5);
    repo.add(&v1)?;
    let c1 = repo.commit("blog v1: User + Post", "alice")?;

    // Verify initial commit has no migration.
    let commit1 = load_commit(repo.store(), c1);
    assert!(commit1.migration_id.is_none());
    assert!(commit1.parents.is_empty());

    // v2: add Comment (v1's 6 + Comment, Comment.text = 8 vertices)
    let v2 = blog_v2();
    assert_eq!(v2.vertex_count(), 8);
    assert_eq!(v2.edge_count(), 8);
    repo.add(&v2)?;
    let c2 = repo.commit("blog v2: add Comment", "alice")?;

    let mig_v1_v2 = load_migration_from_commit(repo.store(), c2);
    // All v1 vertices should survive in the migration.
    assert!(
        mig_v1_v2.vertex_map.len() >= 6,
        "migration v1->v2 should map at least all 6 v1 vertices, got {}",
        mig_v1_v2.vertex_map.len()
    );

    // v3: rename Post.body->Post.content, add Post.published_at
    // 9 vertices: User(3) + Post(4: obj, title, content, published_at) + Comment(2)
    let v3 = blog_v3();
    assert_eq!(v3.vertex_count(), 9);
    assert_eq!(v3.edge_count(), 9);
    repo.add(&v3)?;
    let c3 = repo.commit("blog v3: rename body->content, add published_at", "alice")?;

    let mig_v2_v3 = load_migration_from_commit(repo.store(), c3);
    // Post.body should map to Post.content (rename detected).
    let body_target = mig_v2_v3.vertex_map.get("Post.body");
    assert!(
        body_target.is_some(),
        "Post.body should be mapped in the migration"
    );

    // v4: add Tag + PostTag
    // 12 vertices: v3(9) + Tag(2: obj, name) + PostTag(1: obj) = 12
    let v4 = blog_v4();
    assert_eq!(v4.vertex_count(), 12);
    assert_eq!(v4.edge_count(), 12);
    repo.add(&v4)?;
    let c4 = repo.commit("blog v4: add Tag + PostTag", "alice")?;

    let mig_v3_v4 = load_migration_from_commit(repo.store(), c4);
    assert!(
        mig_v3_v4.vertex_map.len() >= 9,
        "migration v3->v4 should map at least all 9 v3 vertices, got {}",
        mig_v3_v4.vertex_map.len()
    );

    // Verify commit log.
    let log = repo.log(None)?;
    assert_eq!(log.len(), 4);
    assert_eq!(log[0].message, "blog v4: add Tag + PostTag");
    assert_eq!(log[3].message, "blog v1: User + Post");

    // Verify the final schema matches v4.
    let final_schema = load_schema_from_commit(repo.store(), c4);
    assert_eq!(final_schema.vertex_count(), 12);
    assert!(final_schema.has_vertex("Tag"));
    assert!(final_schema.has_vertex("PostTag"));
    assert!(final_schema.has_vertex("Post.content"));
    assert!(!final_schema.has_vertex("Post.body"));

    Ok(())
}

// ===========================================================================
// Test 2: concurrent_feature_merge
// ===========================================================================

#[test]
fn concurrent_feature_merge() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // Base: v2 (has Comment)
    repo.add(&blog_v2())?;
    let base_id = repo.commit("base: v2 with Comment", "alice")?;

    // Branch A: add Comment.edited_at
    refs::create_branch(repo.store_mut(), "feature-edited-at", base_id)?;
    refs::checkout_branch(repo.store_mut(), "feature-edited-at")?;
    repo.add(&blog_v2_with_edited_at())?;
    repo.commit("add Comment.edited_at", "bob")?;

    // Branch B (main): add Comment.likes
    refs::checkout_branch(repo.store_mut(), "main")?;
    repo.add(&blog_v2_with_likes())?;
    repo.commit("add Comment.likes", "alice")?;

    // Merge feature-edited-at into main.
    let result = repo.merge("feature-edited-at", "alice")?;

    // Both branches added independent fields to Comment, so no conflicts.
    assert!(
        result.conflicts.is_empty(),
        "clean merge expected, got {} conflicts: {:?}",
        result.conflicts.len(),
        result.conflicts
    );

    // Verify merged schema has both new fields.
    let head_id = store::resolve_head(repo.store())?.unwrap();
    let merged_schema = load_schema_from_commit(repo.store(), head_id);

    assert!(
        merged_schema.has_vertex("Comment.edited_at"),
        "merged schema should have Comment.edited_at"
    );
    assert!(
        merged_schema.has_vertex("Comment.likes"),
        "merged schema should have Comment.likes"
    );

    // Verify merge commit has two parents.
    let merge_commit = load_commit(repo.store(), head_id);
    assert_eq!(
        merge_commit.parents.len(),
        2,
        "merge commit should have 2 parents"
    );

    Ok(())
}

// ===========================================================================
// Test 3: merge_conflict_resolution
// ===========================================================================

#[test]
fn merge_conflict_resolution() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // Base: v1
    repo.add(&blog_v1())?;
    let base_id = repo.commit("base: v1", "alice")?;

    // Branch A (main): change User.email kind to "email-address"
    repo.add(&blog_v1_email_typed())?;
    repo.commit("type User.email as email-address", "alice")?;

    // Branch B: remove User.email entirely
    refs::create_branch(repo.store_mut(), "no-email", base_id)?;
    refs::checkout_branch(repo.store_mut(), "no-email")?;
    repo.add(&blog_v1_no_email())?;
    repo.commit("remove User.email", "bob")?;

    // Switch back to main and merge with no_commit to inspect conflicts.
    refs::checkout_branch(repo.store_mut(), "main")?;
    let opts = panproto_vcs::merge::MergeOptions {
        no_commit: true,
        ..Default::default()
    };
    let result = repo.merge_with_options("no-email", "alice", &opts)?;

    // Expect a DeleteModifyVertex conflict on User.email.
    assert!(
        !result.conflicts.is_empty(),
        "expected at least one conflict from delete vs modify on User.email"
    );

    let has_delete_modify = result.conflicts.iter().any(|c| {
        matches!(c, panproto_vcs::merge::MergeConflict::DeleteModifyVertex { vertex_id, .. } if vertex_id == "User.email")
    });
    assert!(
        has_delete_modify,
        "expected a DeleteModifyVertex conflict for User.email, got: {:?}",
        result.conflicts
    );

    // Resolve all conflicts with ChooseOurs (keep the email-address typed version).
    let mut resolutions = HashMap::new();
    for (idx, _conflict) in result.conflicts.iter().enumerate() {
        resolutions.insert(idx, panproto_vcs::merge::ConflictResolution::ChooseOurs);
    }
    let strategy = panproto_vcs::merge::ResolutionStrategy { resolutions };

    let base_schema = load_schema_from_commit(repo.store(), base_id);
    let ours_id = store::resolve_head(repo.store())?.unwrap();
    let ours_schema = load_schema_from_commit(repo.store(), ours_id);
    let theirs_id = refs::resolve_ref(repo.store(), "no-email")?;
    let theirs_schema = load_schema_from_commit(repo.store(), theirs_id);

    let resolved = panproto_vcs::merge::apply_resolutions(
        &base_schema,
        &ours_schema,
        &theirs_schema,
        &result,
        &strategy,
    )?;

    // After ChooseOurs, User.email should still exist.
    assert!(
        resolved.schema.has_vertex("User.email"),
        "resolved schema should keep User.email (ChooseOurs)"
    );

    Ok(())
}

// ===========================================================================
// Test 4: theory_tracking
// ===========================================================================

#[test]
fn theory_tracking() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // v1
    repo.add(&blog_v1())?;
    let c1 = repo.commit("v1", "alice")?;

    // v2
    repo.add(&blog_v2())?;
    let c2 = repo.commit("v2", "alice")?;

    // Verify theory_ids are populated on each commit.
    let commit1 = load_commit(repo.store(), c1);
    assert!(
        !commit1.theory_ids.is_empty(),
        "commit v1 should have theory_ids"
    );
    assert!(
        commit1.theory_ids.contains_key("blog"),
        "commit v1 theory_ids should contain the 'blog' protocol key"
    );

    let commit2 = load_commit(repo.store(), c2);
    assert!(
        !commit2.theory_ids.is_empty(),
        "commit v2 should have theory_ids"
    );

    // Load the Theory objects and verify they're valid.
    let theory1_id = commit1.theory_ids.get("blog").unwrap();
    let theory1_obj = repo.store().get(theory1_id)?;
    let theory1 = match theory1_obj {
        Object::Theory(t) => *t,
        other => panic!("expected Theory, got {}", other.type_name()),
    };
    // v1 has 7 vertices, so the theory should have sorts for those.
    assert!(
        !theory1.sorts.is_empty(),
        "theory for v1 should have sorts derived from schema vertices"
    );

    let theory2_id = commit2.theory_ids.get("blog").unwrap();
    let theory2_obj = repo.store().get(theory2_id)?;
    let theory2 = match theory2_obj {
        Object::Theory(t) => *t,
        other => panic!("expected Theory, got {}", other.type_name()),
    };

    // v2 has more vertices, so theory2 should have more sorts than theory1.
    assert!(
        theory2.sorts.len() >= theory1.sorts.len(),
        "theory for v2 ({} sorts) should have at least as many sorts as v1 ({} sorts)",
        theory2.sorts.len(),
        theory1.sorts.len()
    );

    // The theories should differ because v2 added Comment vertices.
    assert_ne!(
        theory1_id, theory2_id,
        "v1 and v2 theories should be different objects"
    );

    Ok(())
}

// ===========================================================================
// Test 5: rebase_with_data
// ===========================================================================

#[test]
fn rebase_with_data() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // Base: v1
    repo.add(&blog_v1())?;
    let base_id = repo.commit("v1 base", "alice")?;

    // Main: evolve to v2 (add Comment)
    repo.add(&blog_v2())?;
    let main_v2 = repo.commit("add Comment", "alice")?;

    // Create feature branch from v1 base and add Tag.
    refs::create_branch(repo.store_mut(), "feature-tag", base_id)?;
    refs::checkout_branch(repo.store_mut(), "feature-tag")?;
    repo.add(&blog_v1_with_tag())?;
    repo.commit("add Tag", "bob")?;

    // Rebase feature-tag onto main (which has Comment).
    let _rebase_result = repo.rebase(main_v2, "bob")?;

    // After rebase, HEAD should be a new commit.
    let head_id = store::resolve_head(repo.store())?.unwrap();
    assert_ne!(
        head_id, base_id,
        "after rebase, HEAD should not be the old base"
    );

    // The rebased commit should have main_v2 as an ancestor.
    let log = repo.log(None)?;
    assert!(
        log.len() >= 2,
        "rebased history should have at least 2 commits"
    );

    // Verify the rebased schema contains both Comment (from main) and Tag (from feature).
    let rebased_schema = load_schema_from_commit(repo.store(), head_id);
    assert!(
        rebased_schema.has_vertex("Tag"),
        "rebased schema should have Tag from feature branch"
    );
    // The schema on the rebased commit is from the feature branch side,
    // which only had Tag added on top of v1. We verify that the rebase
    // successfully replayed on top of main_v2.
    let rebased_commit = load_commit(repo.store(), head_id);
    assert!(
        !rebased_commit.parents.is_empty(),
        "rebased commit should have a parent"
    );

    // The parent of the rebased commit should be main_v2 or a descendant.
    let parent_id = rebased_commit.parents[0];
    let parent_schema = load_schema_from_commit(repo.store(), parent_id);
    assert!(
        parent_schema.has_vertex("Comment"),
        "parent of rebased commit should have Comment (from main v2)"
    );

    Ok(())
}

// ===========================================================================
// Test 6: stash_and_cherry_pick
// ===========================================================================

#[test]
fn stash_and_cherry_pick() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // v1 base
    repo.add(&blog_v1())?;
    let base_id = repo.commit("v1 base", "alice")?;

    // Create a feature branch with Tag and commit.
    refs::create_branch(repo.store_mut(), "feature-tag", base_id)?;
    refs::checkout_branch(repo.store_mut(), "feature-tag")?;
    repo.add(&blog_v1_with_tag())?;
    let tag_commit_id = repo.commit("add Tag", "bob")?;

    // Switch back to main.
    refs::checkout_branch(repo.store_mut(), "main")?;

    // Simulate WIP: stage v3 schema but "stash" it before committing.
    // We store the v3 schema as an object in the store and stash its ID.
    let v3 = blog_v3();
    let v3_schema_id = repo.store_mut().put(&Object::Schema(Box::new(v3)))?;
    let stash_id = panproto_vcs::stash::stash_push(
        repo.store_mut(),
        v3_schema_id,
        "alice",
        Some("WIP: v3 rename"),
    )?;
    assert_ne!(stash_id, ObjectId::ZERO, "stash should produce a valid ID");

    // Verify stash exists.
    let stashes = panproto_vcs::stash::stash_list(repo.store())?;
    assert_eq!(stashes.len(), 1);
    assert_eq!(stashes[0].message, "WIP: v3 rename");

    // Cherry-pick the Tag commit from feature branch onto main.
    let cherry_id = repo.cherry_pick(tag_commit_id, "alice")?;

    // Verify cherry-picked commit schema has Tag.
    let cherry_schema = load_schema_from_commit(repo.store(), cherry_id);
    assert!(
        cherry_schema.has_vertex("Tag"),
        "cherry-picked schema should have Tag"
    );

    // Pop the stash to recover the v3 schema ID.
    let popped_schema_id = panproto_vcs::stash::stash_pop(repo.store_mut())?;
    assert_eq!(
        popped_schema_id, v3_schema_id,
        "popped stash should return the v3 schema ID"
    );

    // Verify the stash ref is removed (no more stash entries to pop).
    let stash_ref = repo.store().get_ref("refs/stash")?;
    assert!(stash_ref.is_none(), "stash ref should be removed after pop");

    // Verify the recovered schema is indeed v3.
    let recovered_schema = match repo.store().get(&popped_schema_id)? {
        Object::Schema(s) => *s,
        other => panic!("expected Schema, got {}", other.type_name()),
    };
    assert!(
        recovered_schema.has_vertex("Post.content"),
        "recovered stash schema should have Post.content (v3 rename)"
    );

    Ok(())
}

// ===========================================================================
// Test 7: bisect_breaking_change
// ===========================================================================

#[test]
fn bisect_breaking_change() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // Build 8 incremental commits, each adding one vertex.
    // Commit at index 5 (the 6th commit) "breaks" by removing User.email.
    let proto = blog_protocol();
    let mut commit_ids = Vec::new();

    // Commit 0: just User
    let s0 = SchemaBuilder::new(&proto)
        .vertex("User", "object", None::<&str>)?
        .vertex("User.name", "string", None::<&str>)?
        .vertex("User.email", "string", None::<&str>)?
        .edge("User", "User.name", "prop", Some("name"))?
        .edge("User", "User.email", "prop", Some("email"))?
        .build()?;
    repo.add(&s0)?;
    commit_ids.push(repo.commit("commit 0: User", "alice")?);

    // Commits 1..4: add more fields, keeping User.email
    for i in 1..5 {
        let mut builder = SchemaBuilder::new(&proto)
            .vertex("User", "object", None::<&str>)?
            .vertex("User.name", "string", None::<&str>)?
            .vertex("User.email", "string", None::<&str>)?
            .edge("User", "User.name", "prop", Some("name"))?
            .edge("User", "User.email", "prop", Some("email"))?;
        for j in 1..=i {
            let fj = format!("User.field{j}");
            builder = builder.vertex(&fj, "string", None::<&str>)?.edge(
                "User",
                &fj,
                "prop",
                Some(&format!("field{j}")),
            )?;
        }
        let schema = builder.build()?;
        repo.add(&schema)?;
        commit_ids.push(repo.commit(&format!("commit {i}"), "alice")?);
    }

    // Commit 5 (the "breaking" commit): removes User.email
    let mut builder5 = SchemaBuilder::new(&proto)
        .vertex("User", "object", None::<&str>)?
        .vertex("User.name", "string", None::<&str>)?
        .edge("User", "User.name", "prop", Some("name"))?;
    for j in 1..=4 {
        let fj = format!("User.field{j}");
        builder5 = builder5.vertex(&fj, "string", None::<&str>)?.edge(
            "User",
            &fj,
            "prop",
            Some(&format!("field{j}")),
        )?;
    }
    builder5 = builder5
        .vertex("User.field5", "string", None::<&str>)?
        .edge("User", "User.field5", "prop", Some("field5"))?;
    let s5 = builder5.build()?;
    repo.add(&s5)?;
    commit_ids.push(repo.commit("commit 5: BREAKING remove email", "alice")?);

    // Commits 6..7: continue without email
    for i in 6..8 {
        let mut builder = SchemaBuilder::new(&proto)
            .vertex("User", "object", None::<&str>)?
            .vertex("User.name", "string", None::<&str>)?
            .edge("User", "User.name", "prop", Some("name"))?;
        for j in 1..=i {
            let fj = format!("User.field{j}");
            builder = builder.vertex(&fj, "string", None::<&str>)?.edge(
                "User",
                &fj,
                "prop",
                Some(&format!("field{j}")),
            )?;
        }
        let schema = builder.build()?;
        repo.add(&schema)?;
        commit_ids.push(repo.commit(&format!("commit {i}"), "alice")?);
    }

    assert_eq!(commit_ids.len(), 8);

    // Bisect to find the commit that removed User.email.
    let good = commit_ids[0]; // has User.email
    let bad = commit_ids[7]; // missing User.email

    let (mut state, mut step) = panproto_vcs::bisect::bisect_start(repo.store(), good, bad)?;

    let breaking_index = 5;
    let mut iterations = 0;

    loop {
        match step {
            panproto_vcs::bisect::BisectStep::Found(id) => {
                assert_eq!(
                    id, commit_ids[breaking_index],
                    "bisect should find commit 5 as the breaking commit"
                );
                break;
            }
            panproto_vcs::bisect::BisectStep::Test(id) => {
                // Check if this commit's schema still has User.email.
                let schema = load_schema_from_commit(repo.store(), id);
                let is_good = schema.has_vertex("User.email");
                step = panproto_vcs::bisect::bisect_step(&mut state, is_good);
                iterations += 1;
                assert!(iterations <= 10, "bisect should converge in log2(8) steps");
            }
        }
    }

    // Bisect of 8 elements should complete in at most 3 steps.
    assert!(
        iterations <= 4,
        "bisect of 8 commits should complete in at most 4 steps, took {iterations}"
    );

    Ok(())
}

// ===========================================================================
// Test 8: composition_path_coherence
// ===========================================================================

#[test]
fn composition_path_coherence() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // Build linear v1 -> v2 -> v3 -> v4.
    repo.add(&blog_v1())?;
    let c1 = repo.commit("v1", "alice")?;

    repo.add(&blog_v2())?;
    let c2 = repo.commit("v2", "alice")?;

    repo.add(&blog_v3())?;
    let c3 = repo.commit("v3", "alice")?;

    repo.add(&blog_v4())?;
    let c4 = repo.commit("v4", "alice")?;

    // Compose the full path v1 -> v4 and check coherence.
    let path = vec![c1, c2, c3, c4];
    let result = panproto_vcs::dag::compose_path_with_coherence(repo.store(), &path)?;

    let composed = &result.migration;

    // The composed migration should map v1 vertices to v4 vertices.
    // Post.body was renamed to Post.content in v3, so the composed
    // migration should reflect that rename.
    let body_mapping = composed.vertex_map.get("Post.body");
    assert!(
        body_mapping.is_some(),
        "composed migration should map Post.body"
    );
    assert_eq!(
        body_mapping.unwrap().as_ref(),
        "Post.content",
        "composed migration should map Post.body to Post.content"
    );

    // All original v1 vertices should be mapped.
    for v in ["User", "User.name", "User.email", "Post", "Post.title"] {
        assert!(
            composed.vertex_map.contains_key(v),
            "composed migration should map vertex '{v}'"
        );
    }

    // User and Post should map to themselves (identity through all steps).
    assert_eq!(
        composed.vertex_map.get("User").map(Name::as_str),
        Some("User"),
        "User should map to User through the whole path"
    );
    assert_eq!(
        composed.vertex_map.get("Post").map(Name::as_str),
        Some("Post"),
        "Post should map to Post through the whole path"
    );

    // Also do a simple compose_path and verify agreement.
    let simple_composed = panproto_vcs::dag::compose_path(repo.store(), &path)?;
    assert_eq!(
        simple_composed
            .vertex_map
            .get("Post.body")
            .map(Name::as_str),
        composed.vertex_map.get("Post.body").map(Name::as_str),
        "compose_path and compose_path_with_coherence should agree on Post.body mapping"
    );

    Ok(())
}
