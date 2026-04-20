//! XRPC request/response serialization benchmarks using real commit shapes.
//!
//! Runs against a local `wiremock` server that returns a realistic
//! `listCommits` payload — no network, no secrets, purely in-process.

#![allow(clippy::expect_used)]

use panproto_xrpc::{CommitEntry, CommitIdentity, ListCommitsResult, NodeClient};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn main() {
    divan::main();
}

fn real_commit_entries() -> Vec<CommitEntry> {
    // Shape matches real panproto-vcs commit entries: 64-hex OIDs (blake3),
    // parent OIDs, author identity, epoch timestamps.
    (0u64..5)
        .map(|i| CommitEntry {
            oid: format!(
                "{:064x}",
                u128::from(i) * 0x0123_4567_89ab_cdef_0123_4567_89ab_cdef_u128
            ),
            parents: if i == 0 {
                vec![]
            } else {
                vec![format!(
                    "{:064x}",
                    u128::from(i - 1) * 0x0123_4567_89ab_cdef_0123_4567_89ab_cdef_u128
                )]
            },
            summary: format!("adopt app.bsky.feed.post v{i}"),
            message: format!(
                "adopt app.bsky.feed.post v{i}\n\nAuto-derived migration from the previous version."
            ),
            author: CommitIdentity {
                name: "bench".into(),
                email: Some("bench@panproto.dev".into()),
            },
            committer: None,
            timestamp: 1_713_500_000 + i * 3600,
            tree_oid: Some(format!(
                "{:064x}",
                u128::from(i + 100) * 0x0123_4567_89ab_cdef_0123_4567_89ab_cdef_u128
            )),
        })
        .collect()
}

#[divan::bench]
fn serialize_list_commits_result(bencher: divan::Bencher) {
    let commits = real_commit_entries();
    let count = commits.len() as u64;
    let result = ListCommitsResult {
        commits,
        count,
        start: Some("0000000000000000000000000000000000000000000000000000000000000000".into()),
    };
    bencher.bench(|| serde_json::to_vec(&result));
}

#[divan::bench]
fn deserialize_list_commits_result(bencher: divan::Bencher) {
    let commits = real_commit_entries();
    let count = commits.len() as u64;
    let result = ListCommitsResult {
        commits,
        count,
        start: Some("0000000000000000000000000000000000000000000000000000000000000000".into()),
    };
    let bytes = serde_json::to_vec(&result).expect("serialize");
    bencher.bench(|| serde_json::from_slice::<ListCommitsResult>(&bytes));
}

/// Full client round-trip: boot a wiremock server, construct a `NodeClient`,
/// call `list_commits`, and tear down. Measures the encode/HTTP/decode
/// path end-to-end but purely in-process (no real network).
#[divan::bench]
fn list_commits_roundtrip_mock_server(bencher: divan::Bencher) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let commits = real_commit_entries();
    let count = commits.len() as u64;
    let result = ListCommitsResult {
        commits,
        count,
        start: None,
    };
    let body = serde_json::to_vec(&result).expect("serialize");

    let (server_url, _server_guard) = rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body.clone(), "application/json"))
            .mount(&server)
            .await;
        (server.uri(), server)
    });

    let client = NodeClient::new(&server_url, "did:plc:example", "bench.repo");
    bencher.bench_local(|| {
        rt.block_on(async { client.list_commits(None, None).await.expect("list") })
    });
}
