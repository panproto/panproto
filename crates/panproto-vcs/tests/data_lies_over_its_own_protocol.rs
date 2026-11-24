//! A committed data set is data over a schema and a protocol, and carries the
//! identity of both: what it means is the same at every commit that holds it,
//! and nothing done to commits makes it unreadable.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use panproto_schema::{ConstraintSemantics, LengthUnit, Protocol, Schema, SchemaBuilder};
use panproto_vcs::environment::load_environment;
use panproto_vcs::{
    AddDataOptions, AddOptions, CommitObject, CommitOptions, DataSetObject, Object, ObjectId,
    ProtocolEnvironment, Repository, Store, VcsError, refs,
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

/// A repository whose one commit records `schema(&["a"])` and `protocol`.
fn repository(protocol: &Protocol) -> (tempfile::TempDir, Repository, ObjectId) {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.add(&schema(&["a"])).unwrap();
    repo.add_protocol(protocol).unwrap();
    let id = repo.commit("schema", "alice").unwrap();
    (dir, repo, id)
}

fn annotation() -> Protocol {
    protocol(ConstraintSemantics::Annotation)
}

fn checked() -> Protocol {
    protocol(max_length())
}

fn environment_at(repo: &Repository, id: ObjectId) -> ProtocolEnvironment {
    load_environment(repo.store(), commit_at(repo, id).environment_id).unwrap()
}

fn head(repo: &Repository) -> ObjectId {
    repo.log_with_ids(Some(1)).unwrap()[0].0
}

fn stage(repo: &mut Repository, dir: &Path, text: &str) -> Result<(), VcsError> {
    let path = write(dir, &format!(r#"{{"a": "{text}"}}"#));
    repo.add_data(&path, Some(text)).map(|_| ())
}

/// The value of `a` in every record at `reference`.
fn texts(repo: &Repository, reference: &str) -> Vec<String> {
    repo.decoded_data_at(reference)
        .unwrap()
        .into_iter()
        .flat_map(|set| set.records)
        .map(|record| record["a"].as_str().unwrap().to_owned())
        .collect()
}

fn branch(repo: &mut Repository, name: &str, at: ObjectId) {
    refs::create_branch(repo.store_mut(), name, at).unwrap();
    refs::checkout_branch(repo.store_mut(), name).unwrap();
}

fn checkout(repo: &mut Repository, name: &str) {
    refs::checkout_branch(repo.store_mut(), name).unwrap();
}

/// Whether data too long for `maxLength` can be staged now.
fn long_is_accepted(repo: &mut Repository, dir: &Path) -> bool {
    let accepted = stage(repo, dir, "abcdef").is_ok();
    repo.clear_index().unwrap();
    accepted
}

// ---------------------------------------------------------------------------
// Reading is a function of the data set alone
// ---------------------------------------------------------------------------

#[test]
fn a_data_set_records_its_protocol() {
    let (dir, mut repo, first) = repository(&checked());
    stage(&mut repo, dir.path(), "abc").unwrap();
    repo.commit("data", "alice").unwrap();
    let set = repo.data_at("HEAD").unwrap().remove(0);
    assert_eq!(set.protocol_id, environment_at(&repo, first).get("local"));
    assert!(matches!(
        repo.store().get(&set.protocol_id.unwrap()).unwrap(),
        Object::Protocol(found) if *found == checked()
    ));
}

#[test]
fn a_built_in_protocol_is_recorded_by_content_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let descriptor = panproto_protocols::registry::descriptor("openapi").unwrap();
    let built_in = (descriptor.protocol)();
    let schema = SchemaBuilder::new(&built_in)
        .vertex("r", "object", None)
        .unwrap()
        .build()
        .unwrap();
    repo.add(&schema).unwrap();
    let path = write(dir.path(), "{}");
    repo.add_data(&path, None).unwrap();
    let data = repo.read_index().unwrap().staged_data.remove(0).data_id;
    let Object::DataSet(DataSetObject {
        protocol_id: Some(id),
        ..
    }) = repo.store().get(&data).unwrap()
    else {
        panic!("the data set records no protocol");
    };
    assert!(matches!(
        repo.store().get(&id).unwrap(),
        Object::Protocol(found) if *found == built_in
    ));
}

#[test]
fn redefining_a_protocol_changes_no_committed_data() {
    let (dir, mut repo, _) = repository(&annotation());
    stage(&mut repo, dir.path(), "abcdef").unwrap();
    let first = repo.commit("long", "alice").unwrap();

    repo.add_protocol(&checked()).unwrap();
    repo.commit("checked", "alice").unwrap();
    assert!(!long_is_accepted(&mut repo, dir.path()));
    assert_eq!(texts(&repo, &first.to_string()), ["abcdef"]);
}

#[test]
fn restaging_a_protocol_after_data_changes_no_staged_data() {
    let (dir, mut repo, _) = repository(&annotation());
    stage(&mut repo, dir.path(), "abcdef").unwrap();
    repo.add_protocol(&checked()).unwrap();
    stage(&mut repo, dir.path(), "abc").unwrap();
    repo.commit("both", "alice").unwrap();
    assert_eq!(texts(&repo, "HEAD"), ["abcdef", "abc"]);
}

#[test]
fn amending_a_protocol_changes_no_committed_data() {
    let (dir, mut repo, _) = repository(&annotation());
    stage(&mut repo, dir.path(), "abcdef").unwrap();
    repo.commit("long", "alice").unwrap();
    repo.add_protocol(&checked()).unwrap();
    let amended = repo.amend("long", "alice").unwrap();
    assert_eq!(texts(&repo, "HEAD"), ["abcdef"]);
    assert_eq!(
        environment_at(&repo, amended).get("local"),
        Some(panproto_vcs::hash::hash_protocol(&checked()).unwrap())
    );
}

#[test]
fn a_handle_that_staged_nothing_reads_and_stages() {
    let (dir, mut repo, _) = repository(&checked());
    stage(&mut repo, dir.path(), "abc").unwrap();
    repo.commit("data", "alice").unwrap();
    let mut reopened = Repository::open(dir.path()).unwrap();
    assert_eq!(texts(&reopened, "HEAD"), ["abc"]);
    assert!(!long_is_accepted(&mut reopened, dir.path()));
    stage(&mut reopened, dir.path(), "abc").unwrap();
}

#[test]
fn collection_keeps_what_data_and_environments_name() {
    let (dir, mut repo, _) = repository(&annotation());
    stage(&mut repo, dir.path(), "abcdef").unwrap();
    repo.commit("long", "alice").unwrap();
    repo.add_protocol(&checked()).unwrap();
    repo.commit("checked", "alice").unwrap();
    repo.gc().unwrap();
    assert_eq!(texts(&repo, "HEAD~1"), ["abcdef"]);
    assert!(!long_is_accepted(&mut repo, dir.path()));
}

/// Data committed before data sets recorded a protocol keeps its identity and
/// is read against its schema alone.
#[test]
fn data_without_a_recorded_protocol_is_read_against_its_schema() {
    let (dir, mut repo, _) = repository(&annotation());
    stage(&mut repo, dir.path(), "abcdef").unwrap();
    let mut index = repo.read_index().unwrap();
    let Object::DataSet(set) = repo.store().get(&index.staged_data[0].data_id).unwrap() else {
        panic!("not a data set");
    };
    let old = DataSetObject {
        protocol_id: None,
        ..set
    };
    index.staged_data[0].data_id = repo.store_mut().put(&Object::DataSet(old)).unwrap();
    repo.write_index(&index).unwrap();
    repo.commit("old", "alice").unwrap();
    repo.add_protocol(&checked()).unwrap();
    repo.commit("checked", "alice").unwrap();
    assert_eq!(texts(&repo, "HEAD~1"), ["abcdef"]);
}

#[test]
fn unverified_data_is_still_checked_when_read() {
    let (dir, mut repo, _) = repository(&checked());
    let path = write(dir.path(), r#"{"a": "abcdef"}"#);
    let unverified = AddDataOptions {
        skip_verify: true,
        ..AddDataOptions::default()
    };
    repo.add_data_with_options(&path, None, &unverified)
        .unwrap();
    repo.commit_with_options("data", "alice", &CommitOptions { skip_verify: true })
        .unwrap();
    assert!(matches!(
        repo.decoded_data_at("HEAD"),
        Err(VcsError::DataValidationFailed { .. })
    ));
}

// ---------------------------------------------------------------------------
// Environments
// ---------------------------------------------------------------------------

#[test]
fn a_commit_records_every_protocol_and_later_commits_keep_them() {
    let (_dir, mut repo, first) = repository(&checked());
    let elsewhere = Protocol {
        name: "elsewhere".into(),
        ..annotation()
    };
    repo.add_protocol(&elsewhere).unwrap();
    let second = repo.commit("elsewhere", "alice").unwrap();
    repo.add(&schema(&["a", "b"])).unwrap();
    let third = repo.commit("b", "alice").unwrap();
    let names = |id| -> Vec<String> {
        environment_at(&repo, id)
            .iter()
            .map(|(name, _)| name.to_owned())
            .collect()
    };
    assert_eq!(names(first), ["local"]);
    assert_eq!(names(second), ["elsewhere", "local"]);
    assert_eq!(names(third), ["elsewhere", "local"]);
}

/// Branches `main` and `side` from a commit under `base`, each adding one
/// property; each branch's protocol is staged if it has one, and then a
/// record `text` is committed on it. HEAD is on `main`.
fn diverged(
    base: &Protocol,
    [ours, theirs]: [(Option<Protocol>, &str); 2],
) -> (tempfile::TempDir, Repository) {
    let (dir, mut repo, first) = repository(base);
    branch(&mut repo, "side", first);
    let advance = |repo: &mut Repository, property, (protocol, text): (Option<Protocol>, _)| {
        if let Some(protocol) = protocol {
            repo.add_protocol(&protocol).unwrap();
        }
        repo.add(&schema(&["a", property])).unwrap();
        stage(repo, dir.path(), text).unwrap();
        repo.commit(property, "alice").unwrap();
    };
    advance(&mut repo, "c", theirs);
    checkout(&mut repo, "main");
    advance(&mut repo, "b", ours);
    (dir, repo)
}

#[test]
fn a_merge_takes_the_change_either_branch_made() {
    for change in 0..2 {
        let mut sides = [(None, "abc"), (None, "xyz")];
        sides[change] = (Some(annotation()), "abc");
        let (dir, mut repo) = diverged(&checked(), sides);
        assert!(repo.merge("side", "alice").unwrap().conflicts.is_empty());
        assert!(long_is_accepted(&mut repo, dir.path()), "{change}");
        assert_eq!(repo.log(Some(1)).unwrap()[0].parents.len(), 2);
    }
}

#[test]
fn a_merge_reads_and_transports_both_branches_data() {
    let sides = [(None, "abc"), (Some(annotation()), "abcdef")];
    let (_dir, mut repo) = diverged(&checked(), sides);
    repo.merge("side", "alice").unwrap();
    let mut found = texts(&repo, "HEAD");
    found.sort();
    assert_eq!(found, ["abc", "abcdef"]);
}

#[test]
fn a_merge_refuses_protocols_that_have_none() {
    let bytes = protocol(ConstraintSemantics::MaxLength(LengthUnit::Utf8Bytes));
    let (_dir, mut repo) = diverged(
        &checked(),
        [(Some(bytes.clone()), "abc"), (Some(annotation()), "xyz")],
    );
    let before = head(&repo);
    match repo.merge("side", "alice") {
        Err(VcsError::ProtocolConflicts { conflicts }) => {
            let id = |protocol| panproto_vcs::hash::hash_protocol(protocol).unwrap();
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].name, "local");
            assert_eq!(conflicts[0].ours, id(&bytes));
            assert_eq!(conflicts[0].theirs, id(&annotation()));
        }
        other => panic!("expected ProtocolConflicts, got {other:?}"),
    }
    assert_eq!(head(&repo), before);

    // Agreeing on the name resolves it.
    repo.add_protocol(&annotation()).unwrap();
    repo.commit("agree", "alice").unwrap();
    repo.merge("side", "alice").unwrap();
    assert_eq!(texts(&repo, "HEAD").len(), 2);
}

/// Transported data must be a record of the schema it is transported to,
/// under the protocol that schema now means.
#[test]
fn a_merge_refuses_data_the_merged_protocol_rejects() {
    let sides = [(Some(checked()), "abc"), (None, "abcdef")];
    let (_dir, mut repo) = diverged(&annotation(), sides);
    let before = head(&repo);
    assert!(matches!(
        repo.merge("side", "alice"),
        Err(VcsError::DataMigrationFailed { .. })
    ));
    assert_eq!(head(&repo), before);
}

#[test]
fn a_replay_merges_the_environment_over_the_commits_parent() {
    for cherry_pick in [true, false] {
        // The tip changed the protocol; the replayed commit did not.
        let (dir, mut repo) = diverged(&checked(), [(Some(annotation()), "abc"), (None, "xyz")]);
        let tip = head(&repo);
        checkout(&mut repo, "side");
        let commit = head(&repo);
        if cherry_pick {
            checkout(&mut repo, "main");
            repo.cherry_pick(commit, "alice").unwrap();
        } else {
            repo.rebase(tip, "alice").unwrap();
        }
        assert!(long_is_accepted(&mut repo, dir.path()), "{cherry_pick}");
        assert_eq!(texts(&repo, "HEAD"), ["xyz"]);
    }
}

#[test]
fn a_replay_carries_the_change_its_commit_made() {
    let (dir, mut repo) = diverged(&checked(), [(None, "abc"), (Some(annotation()), "abcdef")]);
    checkout(&mut repo, "side");
    let commit = head(&repo);
    checkout(&mut repo, "main");
    repo.cherry_pick(commit, "alice").unwrap();
    assert!(long_is_accepted(&mut repo, dir.path()));
    assert_eq!(texts(&repo, "HEAD"), ["abcdef"]);
}

#[test]
fn a_replay_refuses_protocols_that_have_no_merge() {
    let bytes = protocol(ConstraintSemantics::MaxLength(LengthUnit::Utf8Bytes));
    let (_dir, mut repo) = diverged(
        &checked(),
        [(Some(bytes), "abc"), (Some(annotation()), "xyz")],
    );
    checkout(&mut repo, "side");
    let commit = head(&repo);
    checkout(&mut repo, "main");
    assert!(matches!(
        repo.cherry_pick(commit, "alice"),
        Err(VcsError::ProtocolConflicts { .. })
    ));
}

// ---------------------------------------------------------------------------
// Amending
// ---------------------------------------------------------------------------

#[test]
fn amending_records_staged_data_after_the_commits_own() {
    let (dir, mut repo, _) = repository(&checked());
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
    let (_dir, mut repo, _) = repository(&checked());
    repo.add(&Schema {
        protocol: "elsewhere".into(),
        ..schema(&["a", "b"])
    })
    .unwrap();
    let amended = repo.amend("schema", "alice").unwrap();
    assert_eq!(commit_at(&repo, amended).protocol, "elsewhere");
}

#[test]
fn amending_migrates_from_the_parent() {
    let (_dir, mut repo, first) = repository(&checked());
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
    let (dir, mut repo, _) = repository(&checked());
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
    assert!(repo.read_index().unwrap().has_staged());
}
