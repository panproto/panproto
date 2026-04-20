//! Git bridge benchmarks on an in-memory git repository containing a real
//! OpenTelemetry `.proto` file.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::Path;

use panproto_git::import_git_repo;
use panproto_vcs::MemStore;

fn main() {
    divan::main();
}

const TRACE_PROTO: &[u8] = include_bytes!("../../../fixtures/protobuf/trace.proto");

fn build_fixture_repo() -> (tempfile::TempDir, git2::Repository) {
    let tmp = tempfile::tempdir().expect("tmp");
    let repo = git2::Repository::init(tmp.path()).expect("init");
    fs::write(tmp.path().join("trace.proto"), TRACE_PROTO).expect("write");

    let mut index = repo.index().expect("index");
    index.add_path(Path::new("trace.proto")).expect("add");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("tree");
    {
        let tree = repo.find_tree(tree_id).expect("find tree");
        let sig = git2::Signature::now("bench", "bench@panproto.dev").expect("sig");
        repo.commit(Some("HEAD"), &sig, &sig, "vendor trace.proto", &tree, &[])
            .expect("commit");
    }
    (tmp, repo)
}

#[divan::bench]
fn import_single_commit_with_real_proto(bencher: divan::Bencher) {
    let (_tmp, repo) = build_fixture_repo();
    bencher.bench_local(|| {
        let mut store = MemStore::new();
        import_git_repo(&repo, &mut store, "HEAD")
    });
}
