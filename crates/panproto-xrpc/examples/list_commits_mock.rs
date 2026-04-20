//! Drive `NodeClient::list_commits` against a local `wiremock` server
//! returning a realistic panproto-vcs commit listing.

use panproto_xrpc::{CommitEntry, CommitIdentity, ListCommitsResult, NodeClient};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let commits = vec![CommitEntry {
        oid: "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90".into(),
        parents: vec![],
        summary: "adopt app.bsky.feed.post v0".into(),
        message: "adopt app.bsky.feed.post v0".into(),
        author: CommitIdentity {
            name: "example".into(),
            email: Some("example@panproto.dev".into()),
        },
        committer: None,
        timestamp: 1_713_500_000,
        tree_oid: Some("1111111111111111111111111111111111111111111111111111111111111111".into()),
    }];
    let count = commits.len() as u64;
    let body = serde_json::to_vec(&ListCommitsResult {
        commits,
        count,
        start: None,
    })?;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(&server)
        .await;

    let client = NodeClient::new(&server.uri(), "did:plc:example", "demo.repo");
    let result = client.list_commits(None, None).await?;
    println!("listCommits returned {} commit(s)", result.commits.len());
    for c in result.commits {
        println!("  {} — {}", &c.oid[..12], c.summary);
    }
    Ok(())
}
