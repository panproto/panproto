//! Staging establishes what a commit then relies on.
//!
//! Two gaps let unverified material into an ordinary commit. Data was
//! staged without ever being read, so the `schema_id` a data set
//! carries asserted an association nothing had checked. And a stage
//! left `Pending` by `add --skip-verify` was accepted by a default
//! commit, so "not checked" and "checked and passed" reached the same
//! place.
//!
//! Both are the same failure: an operation that established nothing
//! being read as one that established success.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::{Schema, Vertex};
use panproto_vcs::{AddDataOptions, AddOptions, CommitOptions, Repository, VcsError};

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
        entries: Vec::new(),
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

/// A repository with one committed schema, ready for data to be staged
/// against it.
fn repo_with_schema() -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.add(&make_schema(&[("a", "object")])).unwrap();
    repo.commit("schema", "alice").unwrap();
    (dir, repo)
}

fn write(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

// ---------------------------------------------------------------------------
// Data is read before it is staged
// ---------------------------------------------------------------------------

/// The issue's own reproduction: a file that is not JSON at all was
/// staged, recorded as holding one record, and committed.
#[test]
fn a_file_that_is_not_json_cannot_be_staged() {
    let (dir, mut repo) = repo_with_schema();
    let junk = write(&dir, "junk.json", b"NOT JSON AT ALL");

    let result = repo.add_data(&junk, None);
    let Err(VcsError::DataParseFailed { ref path, .. }) = result else {
        panic!("expected a parse failure, got {result:?}");
    };
    assert!(path.ends_with("junk.json"), "the error names the file");
}

/// A failed stage leaves the index alone, so a later `commit` cannot
/// pick up a half-staged set.
#[test]
fn a_refused_data_file_stages_nothing() {
    let (dir, mut repo) = repo_with_schema();
    let junk = write(&dir, "junk.json", b"NOT JSON AT ALL");

    let before = repo.read_index().unwrap();
    let _ = repo.add_data(&junk, None);
    let after = repo.read_index().unwrap();

    assert_eq!(
        after.staged_data.len(),
        before.staged_data.len(),
        "a refused file must not reach the index",
    );
}

/// Well-formed JSON is lifted through the schema and stored in the
/// encoding every reader of a data set decodes, so the association the
/// `schema_id` records is one the repository established.
#[test]
fn staged_data_is_stored_as_records_of_its_schema() {
    let (dir, mut repo) = repo_with_schema();
    let data = write(&dir, "d.json", br"[{}, {}, {}]");

    repo.add_data(&data, Some("k")).unwrap();
    let commit_id = repo.commit("data", "alice").unwrap();

    let sets = repo.data_at(&commit_id.to_string()).unwrap();
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].record_count, 3, "the count is of records read");
    let instances: Vec<panproto_inst::WInstance> = rmp_serde::from_slice(&sets[0].data).unwrap();
    assert_eq!(instances.len(), 3);
}

/// `skip_verify` narrows to the check, not to reading the bytes. A data
/// set says which schema its data belongs to, so bytes that cannot be
/// read as records of that schema cannot be recorded under it at all.
#[test]
fn skipping_the_check_still_requires_the_data_be_readable() {
    let (dir, mut repo) = repo_with_schema();
    let junk = write(&dir, "junk.json", b"NOT JSON AT ALL");

    let result = repo.add_data_with_options(&junk, None, &AddDataOptions { skip_verify: true });
    assert!(
        matches!(result, Err(VcsError::DataParseFailed { .. })),
        "expected a parse failure even with the check skipped, got {result:?}",
    );
}

// ---------------------------------------------------------------------------
// A default commit requires a completed check
// ---------------------------------------------------------------------------

/// Data staged with the check skipped is pending, and a default commit
/// refuses it rather than treating "not checked" as a pass.
#[test]
fn a_default_commit_refuses_pending_data() {
    let (dir, mut repo) = repo_with_schema();
    let data = write(&dir, "d.json", br"[{}]");
    repo.add_data_with_options(&data, None, &AddDataOptions { skip_verify: true })
        .unwrap();

    let result = repo.commit("data", "alice");
    let Err(VcsError::ValidationPending { ref what, .. }) = result else {
        panic!("expected a pending-validation refusal, got {result:?}");
    };
    assert!(what.contains("d.json"), "the error names what is pending");
}

/// The same for a schema, which is the case #269 reports: `add
/// --skip-verify` alone was enough to get unverified material into an
/// ordinary commit.
#[test]
fn a_default_commit_refuses_a_pending_schema() {
    let (_dir, mut repo) = repo_with_schema();
    repo.add_with_options(
        &make_schema(&[("a", "object"), ("b", "string")]),
        &AddOptions { skip_verify: true },
    )
    .unwrap();

    let result = repo.commit("second", "alice");
    assert!(
        matches!(result, Err(VcsError::ValidationPending { .. })),
        "a default commit must refuse a pending schema, got {result:?}",
    );
}

/// The explicit bypass records what it waved through, so the commit is
/// afterwards distinguishable from one whose contents were checked.
#[test]
fn an_explicit_bypass_records_what_it_bypassed() {
    let (dir, mut repo) = repo_with_schema();
    let data = write(&dir, "d.json", br"[{}]");
    repo.add_data_with_options(&data, None, &AddDataOptions { skip_verify: true })
        .unwrap();

    repo.commit_with_options("data", "alice", &CommitOptions { skip_verify: true })
        .unwrap();

    let commit = repo.log(Some(1)).unwrap().remove(0);
    assert_eq!(commit.unverified.len(), 1);
    assert!(
        commit.unverified[0].contains("d.json"),
        "the commit names what was not verified, got {:?}",
        commit.unverified,
    );
}

/// A commit whose staging was checked records nothing bypassed, so the
/// field distinguishes the two cases rather than being always present.
#[test]
fn a_verified_commit_records_no_bypass() {
    let (dir, mut repo) = repo_with_schema();
    let data = write(&dir, "d.json", br"[{}]");
    repo.add_data(&data, None).unwrap();

    repo.commit("data", "alice").unwrap();
    let commit = repo.log(Some(1)).unwrap().remove(0);
    assert!(
        commit.unverified.is_empty(),
        "a checked commit records no bypass, got {:?}",
        commit.unverified,
    );
}
