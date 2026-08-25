# panproto-xrpc

[![crates.io](https://img.shields.io/crates/v/panproto-xrpc.svg)](https://crates.io/crates/panproto-xrpc)
[![docs.rs](https://docs.rs/panproto-xrpc/badge.svg)](https://docs.rs/panproto-xrpc)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

HTTP client for the `dev.panproto.node.*` XRPC endpoints.

## Client construction

`NodeClient::new(base_url, did, repo)` constructs a client without authentication.
`with_token` adds a bearer token. `from_url` parses `panproto://did/repo` and the legacy
`cospan://did/repo` form. The URL form uses `PANPROTO_NODE_URL`, then
`COSPAN_NODE_URL`, and otherwise `https://node.panproto.dev` as the HTTP base.

Read methods include object, ref, HEAD, repository-information, commit-list, and diff
queries. `get_object` recomputes the returned object's content address and rejects an
ID mismatch. Write methods require a token and include `put_object` and compare-and-set
`set_ref`.

## Transfer

`push` lists local refs and objects, calls `negotiate`, uploads the object IDs reported
as needed, and updates remote refs. `pull` negotiates from local object IDs, fetches
needed objects, verifies each object's address through `get_object`, and updates local
refs. The number transferred depends on the negotiation result; the API does not
assume a fixed number of objects for an incremental operation.

## Example

```rust,ignore
use panproto_vcs::FsStore;
use panproto_xrpc::NodeClient;

let client = NodeClient::new(
    "https://node.example.com",
    "did:plc:abc123",
    "schemas",
).with_token(&token);
let mut store = FsStore::open(".")?;

let pushed = client.push(&store).await?;
println!("{}", pushed.objects_pushed);

let pulled = client.pull(&mut store).await?;
println!("{}", pulled.objects_fetched);

let listing = client.list_commits(Some("main"), Some(10)).await?;
for commit in listing.commits {
    println!("{} {}", commit.oid, commit.summary);
}
```

## Public API

| Item | Purpose |
|------|---------|
| `NodeClient` | Endpoint and transfer client |
| `NegotiateResult` | Needed object IDs and remote refs |
| `RepoInfo` | Protocol, default branch, and commit count |
| `CommitEntry`, `ListCommitsResult` | Commit-list response |
| `FileDiff`, `DiffCommitsResult` | Commit-diff response |
| `PushResult`, `PullResult` | Object and ref counts |
| `XrpcError` | URL, auth, transport, codec, node, store, and object-integrity errors |

## License

[MIT](../../LICENSE)
