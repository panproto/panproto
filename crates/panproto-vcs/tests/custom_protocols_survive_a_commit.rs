//! A protocol no registry declares stays attached to the schemas committed
//! under it: every commit made from one that records the protocol records it
//! too, and committed data is read under the protocol of its own commit.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use panproto_schema::{ConstraintSemantics, LengthUnit, Protocol, Schema, SchemaBuilder};
use panproto_vcs::{
    AddDataOptions, AddOptions, CommitObject, CommitOptions, Object, ObjectId, Repository, Store,
    VcsError,
};

/// The protocol "local", giving `maxLength` the meaning `semantics`.
fn protocol(semantics: ConstraintSemantics) -> Protocol {
    Protocol {
        name: "local".into(),
        schema_theory: "ThLocal".into(),
        instance_theory: "ThLocal".into(),
        obj_kinds: vec!["object".into(), "string".into()],
        constraint_sorts: vec!["maxLength".into()],
        constraint_semantics: [("maxLength".to_owned(), semantics)].into(),
        ..Protocol::default()
    }
}

const fn max_length() -> ConstraintSemantics {
    ConstraintSemantics::MaxLength(LengthUnit::Chars)
}

/// An object `r` with one string property per name, each of at most three
/// characters.
fn schema(properties: &[&str]) -> Schema {
    let protocol = protocol(max_length());
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex("r", "object", None)
        .unwrap();
    for name in properties {
        let id = format!("r.{name}");
        builder = builder
            .vertex(&id, "string", None)
            .unwrap()
            .edge("r", &id, "prop", Some(name))
            .unwrap()
            .constraint(&id, "maxLength", "3");
    }
    builder.build().unwrap()
}

fn write(dir: &Path, body: &str) -> std::path::PathBuf {
    let path = dir.join("record.json");
    std::fs::write(&path, body).unwrap();
    path
}

fn commit_at(repo: &Repository, id: ObjectId) -> CommitObject {
    let (_, commit) = repo
        .log_with_ids(None)
        .unwrap()
        .into_iter()
        .find(|(found, _)| *found == id)
        .unwrap();
    commit
}

/// A repository whose one commit records `schema(&["a"])` and the protocol.
fn repository() -> (tempfile::TempDir, Repository, ObjectId) {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.add(&schema(&["a"])).unwrap();
    repo.add_protocol(&protocol(max_length())).unwrap();
    let id = repo.commit("schema", "alice").unwrap();
    (dir, repo, id)
}

#[test]
fn a_data_only_commit_keeps_the_protocol() {
    let (dir, mut repo, first) = repository();
    let path = write(dir.path(), r#"{"a": "abc"}"#);
    repo.add_data(&path, None).unwrap();
    let second = repo.commit("data", "alice").unwrap();
    assert_eq!(
        commit_at(&repo, second).protocol_id,
        commit_at(&repo, first).protocol_id
    );

    // A handle that registers nothing reads and checks under it.
    let mut reopened = Repository::open(dir.path()).unwrap();
    assert_eq!(
        reopened.decoded_data_at("HEAD").unwrap()[0].records,
        [serde_json::json!({"a": "abc"})]
    );
    let long = write(dir.path(), r#"{"a": "abcdef"}"#);
    assert!(matches!(
        reopened.add_data(&long, None),
        Err(VcsError::DataValidationFailed { .. })
    ));
}

/// `schema(properties)` under the protocol "elsewhere".
fn elsewhere(properties: &[&str]) -> Schema {
    Schema {
        protocol: "elsewhere".into(),
        ..schema(properties)
    }
}

/// The protocol "elsewhere", whose `maxLength` constrains nothing.
fn elsewhere_protocol() -> Protocol {
    Protocol {
        name: "elsewhere".into(),
        ..protocol(ConstraintSemantics::Annotation)
    }
}

#[test]
fn a_commit_records_every_protocol_staged_into_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let path = write(dir.path(), r#"{"a": "abc"}"#);
    let mut schema_ids = Vec::new();
    for (schema, protocol) in [
        (schema(&["a"]), protocol(max_length())),
        (elsewhere(&["a"]), elsewhere_protocol()),
    ] {
        schema_ids.push(repo.add(&schema).unwrap().staged.unwrap().schema_id);
        repo.add_protocol(&protocol).unwrap();
        repo.add_data(&path, None).unwrap();
    }
    let both = repo.commit("both", "alice").unwrap();
    assert!(matches!(
        repo.store()
            .get(&commit_at(&repo, both).protocol_id.unwrap())
            .unwrap(),
        Object::ProtocolSet(protocols)
            if protocols.keys().eq(["elsewhere", "local"])
    ));

    // A handle that registers nothing reads and stages under each, and each
    // keeps its own meaning of the constraint.
    let mut reopened = Repository::open(dir.path()).unwrap();
    assert_eq!(reopened.decoded_data_at("HEAD").unwrap().len(), 2);
    let long = write(dir.path(), r#"{"a": "abcdef"}"#);
    let under = |schema_id| AddDataOptions {
        schema_id: Some(schema_id),
        ..AddDataOptions::default()
    };
    assert!(matches!(
        reopened.add_data_with_options(&long, None, &under(schema_ids[0])),
        Err(VcsError::DataValidationFailed { .. })
    ));
    reopened
        .add_data_with_options(&long, None, &under(schema_ids[1]))
        .unwrap();

    // The set and its protocols outlive a collection and the next commit.
    let next = reopened.commit("next", "alice").unwrap();
    reopened.gc().unwrap();
    assert_eq!(
        commit_at(&reopened, next).protocol_id,
        commit_at(&reopened, both).protocol_id
    );
    assert_eq!(reopened.decoded_data_at("HEAD").unwrap().len(), 1);
}

#[test]
fn staging_a_protocol_again_replaces_the_one_of_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.add(&schema(&["a"])).unwrap();
    repo.add_protocol(&protocol(max_length())).unwrap();
    repo.add_protocol(&elsewhere_protocol()).unwrap();
    repo.add_protocol(&protocol(ConstraintSemantics::Annotation))
        .unwrap();
    let long = write(dir.path(), r#"{"a": "abcdef"}"#);
    repo.add_data(&long, None).unwrap();
}

#[test]
fn a_schema_under_another_protocol_keeps_the_recorded_one() {
    let (_dir, mut repo, first) = repository();
    repo.add(&elsewhere(&["a", "b"])).unwrap();
    let id = repo.commit("other", "alice").unwrap();
    assert_eq!(
        commit_at(&repo, id).protocol_id,
        commit_at(&repo, first).protocol_id
    );
}

#[test]
fn committed_data_is_read_under_its_own_commits_protocol() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.add(&schema(&["a"])).unwrap();
    repo.add_protocol(&protocol(ConstraintSemantics::Annotation))
        .unwrap();
    let path = write(dir.path(), r#"{"a": "abcdef"}"#);
    repo.add_data(&path, None).unwrap();
    let first = repo.commit("unchecked sort", "alice").unwrap();

    repo.add_protocol(&protocol(max_length())).unwrap();
    repo.commit("checked sort", "alice").unwrap();

    // Under HEAD's protocol the record is too long.
    assert!(matches!(
        repo.add_data(&path, None),
        Err(VcsError::DataValidationFailed { .. })
    ));
    assert_eq!(
        repo.decoded_data_at(&first.to_string()).unwrap()[0].records,
        [serde_json::json!({"a": "abcdef"})]
    );
}

#[test]
fn committed_data_outlives_an_unrecorded_protocol() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.set_protocol(protocol(max_length()));
    repo.add(&schema(&["a"])).unwrap();
    let path = write(dir.path(), r#"{"a": "abc"}"#);
    repo.add_data(&path, None).unwrap();
    repo.commit("data", "alice").unwrap();

    let mut reopened = Repository::open(dir.path()).unwrap();
    assert_eq!(
        reopened.decoded_data_at("HEAD").unwrap()[0].records,
        [serde_json::json!({"a": "abc"})]
    );
    // Staging under the unresolved name is still refused.
    match reopened.add_data(&path, None) {
        Err(VcsError::DataValidationFailed { reasons, .. }) => {
            assert!(
                reasons
                    .iter()
                    .all(|r| r.ends_with(r#"unknown protocol "local""#)),
                "{reasons:?}"
            );
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// What the schema decides without a protocol is still checked on read.
#[test]
fn committed_data_is_checked_against_the_schema_without_a_protocol() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.set_protocol(protocol(max_length()));
    let mut required = schema(&["a"]);
    let edge = required.edges.keys().next().unwrap().clone();
    required.required.insert("r".into(), vec![edge]);
    repo.add(&required).unwrap();
    let path = write(dir.path(), "{}");
    repo.add_data_with_options(
        &path,
        None,
        &AddDataOptions {
            skip_verify: true,
            ..AddDataOptions::default()
        },
    )
    .unwrap();
    repo.commit_with_options("data", "alice", &CommitOptions { skip_verify: true })
        .unwrap();

    let reopened = Repository::open(dir.path()).unwrap();
    assert!(matches!(
        reopened.decoded_data_at("HEAD"),
        Err(VcsError::DataValidationFailed { .. })
    ));
}

/// Staging refuses an undeclared attribute; a record that has one already
/// reads back with it.
#[test]
fn a_committed_undeclared_attribute_is_read_back() {
    let (dir, mut repo, _) = repository();
    let path = write(dir.path(), r#"{"a": "abc", "extra": 1}"#);
    assert!(matches!(
        repo.add_data(&path, None),
        Err(VcsError::DataValidationFailed { .. })
    ));
    repo.add_data_with_options(
        &path,
        None,
        &AddDataOptions {
            skip_verify: true,
            ..AddDataOptions::default()
        },
    )
    .unwrap();
    repo.commit_with_options("data", "alice", &CommitOptions { skip_verify: true })
        .unwrap();
    assert_eq!(
        repo.decoded_data_at("HEAD").unwrap()[0].records,
        [serde_json::json!({"a": "abc", "extra": 1})]
    );
}

/// Two branches from a commit recording the protocol, each adding one
/// property; HEAD is on `main`. Returns the tips of `main` and `side`.
fn diverged() -> (tempfile::TempDir, Repository, ObjectId, ObjectId) {
    let (dir, mut repo, first) = repository();
    panproto_vcs::refs::create_branch(repo.store_mut(), "side", first).unwrap();
    panproto_vcs::refs::checkout_branch(repo.store_mut(), "side").unwrap();
    repo.add(&schema(&["a", "c"])).unwrap();
    let theirs = repo.commit("side", "bob").unwrap();
    panproto_vcs::refs::checkout_branch(repo.store_mut(), "main").unwrap();
    repo.add(&schema(&["a", "b"])).unwrap();
    let ours = repo.commit("main", "alice").unwrap();
    (dir, repo, ours, theirs)
}

#[test]
fn a_merge_commit_keeps_the_protocol() {
    let (_dir, mut repo, ours, _) = diverged();
    let recorded = commit_at(&repo, ours).protocol_id;
    assert!(recorded.is_some());
    repo.merge("side", "alice").unwrap();
    let head = repo.log(Some(1)).unwrap().remove(0);
    assert_eq!(head.parents.len(), 2);
    assert_eq!(head.protocol_id, recorded);
}

#[test]
fn a_cherry_picked_commit_keeps_the_protocol() {
    let (_dir, mut repo, ours, theirs) = diverged();
    let recorded = commit_at(&repo, ours).protocol_id;
    let picked = repo.cherry_pick(theirs, "alice").unwrap();
    assert_eq!(commit_at(&repo, picked).protocol_id, recorded);
}

#[test]
fn a_cherry_picked_commit_with_options_keeps_the_protocol() {
    let (_dir, mut repo, ours, theirs) = diverged();
    let recorded = commit_at(&repo, ours).protocol_id;
    let registered = std::collections::HashMap::<String, Protocol>::new();
    let picked = panproto_vcs::cherry_pick::cherry_pick_with_options(
        repo.store_mut(),
        theirs,
        "alice",
        &panproto_vcs::cherry_pick::CherryPickOptions::default(),
        &registered,
    )
    .unwrap();
    assert_eq!(commit_at(&repo, picked).protocol_id, recorded);
}

#[test]
fn a_rebased_commit_keeps_the_protocol() {
    let (_dir, mut repo, ours, theirs) = diverged();
    let recorded = commit_at(&repo, ours).protocol_id;
    let rebased = repo.rebase(theirs, "alice").unwrap();
    assert_eq!(commit_at(&repo, rebased).protocol_id, recorded);
}

#[test]
fn amending_records_a_newly_staged_protocol() {
    let (_dir, mut repo, first) = repository();
    let before = commit_at(&repo, first).protocol_id;
    repo.add_protocol(&protocol(ConstraintSemantics::Annotation))
        .unwrap();
    let amended = repo.amend("schema", "alice").unwrap();
    let after = commit_at(&repo, amended).protocol_id;
    assert!(after.is_some());
    assert_ne!(after, before);
}

#[test]
fn amending_records_staged_data_after_the_commits_own() {
    let (dir, mut repo, _) = repository();
    repo.add_data(&write(dir.path(), r#"{"a": "one"}"#), Some("1"))
        .unwrap();
    repo.commit("data", "alice").unwrap();
    repo.add_data(&write(dir.path(), r#"{"a": "two"}"#), Some("2"))
        .unwrap();
    repo.amend("data", "alice").unwrap();
    let records: Vec<_> = repo
        .decoded_data_at("HEAD")
        .unwrap()
        .into_iter()
        .flat_map(|set| set.records)
        .collect();
    assert_eq!(
        records,
        [
            serde_json::json!({"a": "one"}),
            serde_json::json!({"a": "two"})
        ]
    );
    assert!(!repo.read_index().unwrap().has_staged());
}

#[test]
fn amending_names_the_staged_schemas_protocol() {
    let (_dir, mut repo, _) = repository();
    repo.add(&elsewhere(&["a", "b"])).unwrap();
    let amended = repo.amend("schema", "alice").unwrap();
    assert_eq!(commit_at(&repo, amended).protocol, "elsewhere");
}

#[test]
fn amending_migrates_from_the_parent() {
    let (_dir, mut repo, first) = repository();
    repo.add(&schema(&["a", "b"])).unwrap();
    repo.commit("b", "alice").unwrap();
    repo.add(&schema(&["a", "c"])).unwrap();
    let amended = repo.amend("c", "alice").unwrap();
    let commit = commit_at(&repo, amended);
    assert_eq!(commit.parents, [first]);
    let Object::Migration { src, tgt, .. } =
        repo.store().get(&commit.migration_id.unwrap()).unwrap()
    else {
        panic!("the migration id names something else");
    };
    let flat = |id| match repo.store().get(id).unwrap() {
        Object::FlatSchema(schema) => schema,
        other => panic!("expected a flat schema, got {}", other.type_name()),
    };
    assert!(flat(&src).has_vertex("r.a") && !flat(&src).has_vertex("r.b"));
    assert!(flat(&tgt).has_vertex("r.c") && !flat(&tgt).has_vertex("r.b"));
}

#[test]
fn amending_refuses_what_a_commit_refuses() {
    let (dir, mut repo, first) = repository();
    let long = write(dir.path(), r#"{"a": "abcdef"}"#);
    repo.add_data_with_options(
        &long,
        None,
        &AddDataOptions {
            skip_verify: true,
            ..AddDataOptions::default()
        },
    )
    .unwrap();
    assert!(matches!(
        repo.amend("data", "alice"),
        Err(VcsError::ValidationPending { .. })
    ));
    repo.clear_index().unwrap();
    repo.add_with_options(&schema(&["a", "b"]), &AddOptions { skip_verify: true })
        .unwrap();
    assert!(matches!(
        repo.amend("schema", "alice"),
        Err(VcsError::ValidationPending { .. })
    ));
    // Nothing moved and the stage is kept.
    assert_eq!(repo.log(None).unwrap().len(), 1);
    assert_eq!(
        commit_at(&repo, first).protocol_id,
        repo.log(None).unwrap()[0].protocol_id
    );
    assert!(repo.read_index().unwrap().has_staged());
}
