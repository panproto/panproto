//! VCS benchmarks: commit a chain of real AT Proto Lexicons into a repository.

#![allow(clippy::expect_used)]

use panproto_protocols::web_document::atproto;
use panproto_schema::Schema;
use panproto_vcs::Repository;

fn main() {
    divan::main();
}

const LEXICONS: &[&str] = &[
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json"),
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.actor.profile.json"),
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.like.json"),
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.graph.follow.json"),
];

fn load(src: &str) -> Schema {
    let v: serde_json::Value = serde_json::from_str(src).expect("json");
    atproto::parse_lexicon(&v).expect("lex")
}

#[divan::bench]
fn init_and_commit_feed_post(bencher: divan::Bencher) {
    let schema = load(LEXICONS[0]);
    bencher.bench_local(|| {
        let tmp = tempfile::tempdir().expect("tmp");
        let mut repo = Repository::init(tmp.path()).expect("init");
        repo.add(&schema).expect("add");
        repo.commit("initial", "bench").expect("commit")
    });
}

#[divan::bench]
fn commit_chain_of_real_lexicons(bencher: divan::Bencher) {
    let schemas: Vec<Schema> = LEXICONS.iter().map(|s| load(s)).collect();
    bencher.bench_local(|| {
        let tmp = tempfile::tempdir().expect("tmp");
        let mut repo = Repository::init(tmp.path()).expect("init");
        for (i, s) in schemas.iter().enumerate() {
            repo.add(s).expect("add");
            repo.commit(&format!("step {i}"), "bench").expect("commit");
        }
    });
}

#[divan::bench]
fn log_chain_of_real_lexicons(bencher: divan::Bencher) {
    let schemas: Vec<Schema> = LEXICONS.iter().map(|s| load(s)).collect();
    let tmp = tempfile::tempdir().expect("tmp");
    let mut repo = Repository::init(tmp.path()).expect("init");
    for (i, s) in schemas.iter().enumerate() {
        repo.add(s).expect("add");
        repo.commit(&format!("step {i}"), "bench").expect("commit");
    }
    bencher.bench_local(|| repo.log(None));
}
