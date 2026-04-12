# panproto-xrpc

[![crates.io](https://img.shields.io/crates/v/panproto-xrpc.svg)](https://crates.io/crates/panproto-xrpc)
[![docs.rs](https://docs.rs/panproto-xrpc/badge.svg)](https://docs.rs/panproto-xrpc)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

XRPC client for pushing and pulling schemas to and from panproto node servers.

## What it does

A panproto node is an HTTP server that stores content-addressed VCS objects (schemas, commits, data) and named refs. This crate implements the `dev.panproto.node.*` XRPC endpoints for talking to those servers. Think of it as the transport layer for `schema push` and `schema pull`, the same way `git-upload-pack` and `git-receive-pack` are the transport layer for `git push` and `git pull`.

The high-level `push()` and `pull()` methods handle the full negotiation flow: before transferring objects, the client and server exchange lists of object IDs they already have (the "have/want" step), so only the objects that are actually missing get sent over the wire. For typical incremental pushes this means just one or two new commits are transferred rather than the full history.

Individual endpoint methods are also exposed for cases where you need finer control: fetch a single object by ID, update a specific ref, list all refs, walk commit history from a ref, or diff two commits on the server.

## Quick example

```rust,ignore
use panproto_xrpc::NodeClient;
use panproto_vcs::FsStore;

let client = NodeClient::new("https://node.example.com")?;
let store = FsStore::open(".panproto")?;

// Push all local commits and refs to the remote.
let result = client.push(&store).await?;
println!("pushed {} objects", result.objects_sent);

// Pull all remote commits and refs into the local store.
let result = client.pull(&mut store).await?;
println!("pulled {} objects", result.objects_received);

// List commits on a branch.
let commits = client.list_commits("main", Some(10)).await?;
for entry in &commits.entries {
    println!("{} {}", &entry.id[..8], entry.message);
}
```

## API overview

| Export | What it does |
|--------|-------------|
| `NodeClient` | HTTP client for a single panproto node; owns the base URL and auth token |
| `NodeClient::push()` | Negotiate and push all local objects and refs to the remote |
| `NodeClient::pull()` | Negotiate and pull all remote objects and refs into the local store |
| `NodeClient::get_object()` | Fetch one content-addressed object by ID |
| `NodeClient::put_object()` | Store one object on the remote |
| `NodeClient::get_ref()` | Resolve a named ref to its current object ID |
| `NodeClient::set_ref()` | Update a named ref (requires auth) |
| `NodeClient::list_refs()` | List all refs on the remote |
| `NodeClient::get_head()` | Get the current HEAD state |
| `NodeClient::negotiate()` | Raw have/want negotiation (used internally by push/pull) |
| `NodeClient::get_repo_info()` | Fetch repository metadata |
| `NodeClient::list_commits()` | Walk commit history from a named ref |
| `NodeClient::diff_commits()` | Get a schema diff between two commits on the server |
| `NegotiateResult` | List of object IDs the server needs |
| `RepoInfo` | Repository metadata (name, default branch, commit count) |
| `PushResult` | Number of objects and refs sent |
| `PullResult` | Number of objects and refs received |
| `ListCommitsResult` | Commit entries with IDs, messages, authors, timestamps |
| `DiffCommitsResult` | Per-file schema diffs between two commits |
| `CommitEntry` | One commit in a `ListCommitsResult` |
| `FileDiff` | One file's schema diff in a `DiffCommitsResult` |
| `XrpcError` | Error variants: HTTP errors, auth failures, object not found |

## License

[MIT](../../LICENSE)
