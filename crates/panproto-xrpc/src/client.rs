//! XRPC client for panproto node VCS operations.
//!
//! Implements the `dev.panproto.node.*` XRPC endpoints for push/pull/clone
//! of panproto-vcs objects between local stores and remote nodes.

use std::fmt::Write as _;

use panproto_vcs::{HeadState, Object, ObjectId, Store};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::XrpcError;

/// A client for communicating with a panproto node's XRPC endpoints.
#[derive(Debug, Clone)]
pub struct NodeClient {
    /// Base URL of the panproto node (e.g. `https://node.panproto.dev`).
    base_url: String,
    /// The DID identifying the repo owner.
    did: String,
    /// The repository name.
    repo: String,
    /// Bearer token for authenticated operations.
    token: Option<String>,
    /// HTTP client.
    http: Client,
}

/// Result of a have/want negotiation.
#[derive(Debug, Serialize, Deserialize)]
pub struct NegotiateResult {
    /// Object IDs the remote needs (for push) or the local needs (for pull).
    pub need: Vec<String>,
    /// Refs the remote has.
    pub refs: Vec<(String, String)>,
}

/// Repository metadata from the node.
#[derive(Debug, Serialize, Deserialize)]
pub struct RepoInfo {
    /// The protocol this repo tracks.
    pub protocol: String,
    /// The default branch name.
    pub default_branch: String,
    /// Number of commits.
    pub commit_count: u64,
}

/// Identity (author or committer) within a commit listing.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitIdentity {
    /// Display name.
    pub name: String,
    /// Email address, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// A single commit entry returned by `listCommits`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitEntry {
    /// Full hex object ID of this commit.
    pub oid: String,
    /// Parent commit OIDs.
    pub parents: Vec<String>,
    /// First line of the commit message.
    pub summary: String,
    /// Full commit message.
    pub message: String,
    /// Author identity.
    pub author: CommitIdentity,
    /// Committer identity (same as author in panproto's current model).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub committer: Option<CommitIdentity>,
    /// Unix timestamp in seconds.
    pub timestamp: u64,
    /// OID of the schema object at this commit (the "tree").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree_oid: Option<String>,
}

/// Response from `dev.panproto.node.listCommits`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListCommitsResult {
    /// Commits in topological + time order (newest first).
    pub commits: Vec<CommitEntry>,
    /// Number of commits returned.
    pub count: u64,
    /// OID of the starting commit (the ref tip), if resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
}

/// A single file's diff entry returned by `diffCommits`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    /// Path of the file in the new tree (or the only path if not renamed).
    pub path: String,
    /// Previous path, if the file was renamed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    /// Change status: `"added"`, `"removed"`, `"modified"`, `"renamed"`, etc.
    pub status: String,
    /// OID of the file blob in the old tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_oid: Option<String>,
    /// OID of the file blob in the new tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_oid: Option<String>,
    /// Lines added.
    pub additions: u64,
    /// Lines removed.
    pub deletions: u64,
    /// Whether this is a binary diff.
    pub binary: bool,
    /// Text diff hunks (populated once panproto tracks file blobs).
    pub hunks: Vec<serde_json::Value>,
    /// Panproto schema-level structural diff, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structural_diff: Option<serde_json::Value>,
}

/// Response from `dev.panproto.node.diffCommits`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffCommitsResult {
    /// OID of the base (old) commit.
    pub from: String,
    /// OID of the head (new) commit.
    pub to: String,
    /// Per-file diff entries.
    pub files: Vec<FileDiff>,
    /// Total lines added across all files.
    pub total_additions: u64,
    /// Total lines removed across all files.
    pub total_deletions: u64,
    /// Number of files changed.
    pub file_count: u64,
}

impl NodeClient {
    /// Create a new client for a panproto node.
    #[must_use]
    pub fn new(base_url: &str, did: &str, repo: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            did: did.to_owned(),
            repo: repo.to_owned(),
            token: None,
            http: Client::new(),
        }
    }

    /// Set the bearer token for authenticated operations.
    #[must_use]
    pub fn with_token(mut self, token: &str) -> Self {
        self.token = Some(token.to_owned());
        self
    }

    /// Parse a `panproto://did/repo` URL into (`base_url`, `did`, `repo`).
    ///
    /// Also accepts the legacy `cospan://` prefix for backward compatibility.
    /// The base URL defaults to `https://node.panproto.dev` unless overridden
    /// by the `PANPROTO_NODE_URL` environment variable (falls back to
    /// `COSPAN_NODE_URL` for backward compatibility).
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError::InvalidUrl`] if the URL format is invalid.
    pub fn from_url(url: &str) -> Result<Self, XrpcError> {
        let path = url
            .strip_prefix("panproto://")
            .or_else(|| url.strip_prefix("cospan://"))
            .ok_or_else(|| {
                XrpcError::InvalidUrl(format!("expected panproto:// or cospan:// prefix: {url}"))
            })?;

        let parts: Vec<&str> = path.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(XrpcError::InvalidUrl(format!(
                "expected panproto://did/repo: {url}"
            )));
        }

        let base = std::env::var("PANPROTO_NODE_URL")
            .or_else(|_| std::env::var("COSPAN_NODE_URL"))
            .unwrap_or_else(|_| "https://node.panproto.dev".to_owned());

        Ok(Self::new(&base, parts[0], parts[1]))
    }

    // ── Read operations (no auth required) ──────────────────────────

    /// Fetch a content-addressed object by ID. Returns msgpack-encoded bytes.
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError`] on network or decode failure.
    pub async fn get_object(&self, id: &ObjectId) -> Result<Object, XrpcError> {
        let url = format!(
            "{}/xrpc/dev.panproto.node.getObject?did={}&repo={}&id={}",
            self.base_url, self.did, self.repo, id
        );
        let resp = self.http.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(XrpcError::NodeError {
                endpoint: "getObject".to_owned(),
                status: status.as_u16(),
                body,
            });
        }
        let bytes = resp.bytes().await?;
        let obj: Object = rmp_serde::from_slice(&bytes)?;
        Ok(obj)
    }

    /// Resolve a named ref to an object ID.
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError`] on network failure or if the ref doesn't exist.
    pub async fn get_ref(&self, ref_name: &str) -> Result<Option<ObjectId>, XrpcError> {
        let url = format!(
            "{}/xrpc/dev.panproto.node.getRef?did={}&repo={}&ref={}",
            self.base_url, self.did, self.repo, ref_name
        );
        let resp = self.http.get(&url).send().await?;
        let status = resp.status();
        if status.as_u16() == 404 {
            return Ok(None);
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(XrpcError::NodeError {
                endpoint: "getRef".to_owned(),
                status: status.as_u16(),
                body,
            });
        }
        let body: serde_json::Value = resp.json().await?;
        let id_str = body["target"]
            .as_str()
            .ok_or_else(|| XrpcError::NodeError {
                endpoint: "getRef".to_owned(),
                status: 200,
                body: "missing target field".to_owned(),
            })?;
        Ok(Some(parse_object_id(id_str)?))
    }

    /// List all refs in the repository.
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError`] on network failure.
    pub async fn list_refs(&self) -> Result<Vec<(String, ObjectId)>, XrpcError> {
        let url = format!(
            "{}/xrpc/dev.panproto.node.listRefs?did={}&repo={}",
            self.base_url, self.did, self.repo
        );
        let resp = self.http.get(&url).send().await?;
        let resp = check_response(resp, "listRefs").await?;
        let body: serde_json::Value = resp.json().await?;
        let refs = body["refs"]
            .as_array()
            .ok_or_else(|| XrpcError::NodeError {
                endpoint: "listRefs".to_owned(),
                status: 200,
                body: "missing refs array".to_owned(),
            })?;
        let mut result = Vec::new();
        for (i, r) in refs.iter().enumerate() {
            let name = r["name"].as_str().ok_or_else(|| XrpcError::NodeError {
                endpoint: "listRefs".to_owned(),
                status: 200,
                body: format!("ref entry {i} missing 'name' field"),
            })?;
            let target = r["target"].as_str().ok_or_else(|| XrpcError::NodeError {
                endpoint: "listRefs".to_owned(),
                status: 200,
                body: format!("ref entry {i} ('{name}') missing 'target' field"),
            })?;
            result.push((name.to_owned(), parse_object_id(target)?));
        }
        Ok(result)
    }

    /// Get the HEAD state of the repository.
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError`] on network failure.
    pub async fn get_head(&self) -> Result<HeadState, XrpcError> {
        let url = format!(
            "{}/xrpc/dev.panproto.node.getHead?did={}&repo={}",
            self.base_url, self.did, self.repo
        );
        let resp = self.http.get(&url).send().await?;
        let resp = check_response(resp, "getHead").await?;
        let body: serde_json::Value = resp.json().await?;
        if let Some(branch) = body["branch"].as_str() {
            Ok(HeadState::Branch(branch.to_owned()))
        } else if let Some(id_str) = body["detached"].as_str() {
            Ok(HeadState::Detached(parse_object_id(id_str)?))
        } else {
            Err(XrpcError::NodeError {
                endpoint: "getHead".to_owned(),
                status: 200,
                body: format!(
                    "unexpected HEAD response: neither 'branch' nor 'detached' field present: {body}"
                ),
            })
        }
    }

    /// Get repository metadata.
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError`] on network failure.
    pub async fn get_repo_info(&self) -> Result<RepoInfo, XrpcError> {
        let url = format!(
            "{}/xrpc/dev.panproto.node.getRepoInfo?did={}&repo={}",
            self.base_url, self.did, self.repo
        );
        let resp = self.http.get(&url).send().await?;
        let resp = check_response(resp, "getRepoInfo").await?;
        let info: RepoInfo = resp.json().await?;
        Ok(info)
    }

    /// List commits reachable from a ref (default: HEAD).
    ///
    /// Returns commits in topological + time order (newest first),
    /// up to `limit` entries (default 50, max 500 on the server).
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError`] on network failure or parse error.
    pub async fn list_commits(
        &self,
        git_ref: Option<&str>,
        limit: Option<u32>,
    ) -> Result<ListCommitsResult, XrpcError> {
        let url = build_list_commits_url(&self.base_url, &self.did, &self.repo, git_ref, limit);
        let resp = self.http.get(&url).send().await?;
        let resp = check_response(resp, "listCommits").await?;
        let result: ListCommitsResult = resp.json().await?;
        Ok(result)
    }

    /// Compute the diff between two commits.
    ///
    /// `from` and `to` are full hex object IDs. `context_lines` controls
    /// how many surrounding lines to include in text hunks (default 3 on
    /// the server).
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError`] on network failure or parse error.
    pub async fn diff_commits(
        &self,
        from: &str,
        to: &str,
        context_lines: Option<u32>,
    ) -> Result<DiffCommitsResult, XrpcError> {
        let url = build_diff_commits_url(
            &self.base_url,
            &self.did,
            &self.repo,
            from,
            to,
            context_lines,
        );
        let resp = self.http.get(&url).send().await?;
        let resp = check_response(resp, "diffCommits").await?;
        let result: DiffCommitsResult = resp.json().await?;
        Ok(result)
    }

    // ── Write operations (auth required) ─────────────────────────────

    /// Store a content-addressed object on the node.
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError::AuthRequired`] if no token is set.
    /// Returns [`XrpcError`] on network or encode failure.
    pub async fn put_object(&self, object: &Object) -> Result<ObjectId, XrpcError> {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| XrpcError::AuthRequired("putObject requires auth".to_owned()))?;

        let url = format!(
            "{}/xrpc/dev.panproto.node.putObject?did={}&repo={}",
            self.base_url, self.did, self.repo
        );
        let body = rmp_serde::to_vec(object)?;
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/msgpack")
            .body(body)
            .send()
            .await?;
        check_status_owned(resp, "putObject").await
    }

    /// Update a named ref on the node.
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError::AuthRequired`] if no token is set.
    pub async fn set_ref(
        &self,
        ref_name: &str,
        old_target: Option<&ObjectId>,
        new_target: &ObjectId,
        protocol: &str,
        commit_count: u64,
    ) -> Result<(), XrpcError> {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| XrpcError::AuthRequired("setRef requires auth".to_owned()))?;

        let url = format!("{}/xrpc/dev.panproto.node.setRef", self.base_url);
        let body = serde_json::json!({
            "did": self.did,
            "repo": self.repo,
            "ref": ref_name,
            "oldTarget": old_target.map(ToString::to_string),
            "newTarget": new_target.to_string(),
            "protocol": protocol,
            "commitCount": commit_count,
        });
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(XrpcError::NodeError {
                endpoint: "setRef".to_owned(),
                status,
                body,
            });
        }
        Ok(())
    }

    /// Run have/want negotiation for efficient object transfer.
    ///
    /// Sends the local object IDs we have and the ref names we want.
    /// Returns the object IDs the other side needs.
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError`] on network failure.
    pub async fn negotiate(
        &self,
        have: &[ObjectId],
        want: &[String],
    ) -> Result<NegotiateResult, XrpcError> {
        let url = format!("{}/xrpc/dev.panproto.node.negotiate", self.base_url);
        let body = serde_json::json!({
            "did": self.did,
            "repo": self.repo,
            "have": have.iter().map(ObjectId::to_string).collect::<Vec<_>>(),
            "want": want,
        });
        let mut req = self.http.post(&url).json(&body);
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(XrpcError::NodeError {
                endpoint: "negotiate".to_owned(),
                status,
                body,
            });
        }
        let result: NegotiateResult = resp.json().await?;
        Ok(result)
    }

    // ── High-level push/pull ─────────────────────────────────────────

    /// Push local objects and refs to the remote node.
    ///
    /// Flow: list local refs, negotiate, putObject for each needed object,
    /// setRef for each ref.
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError`] on any failure in the push pipeline.
    pub async fn push<S: Store>(&self, store: &S) -> Result<PushResult, XrpcError> {
        // List local refs.
        let local_refs = store.list_refs("refs/")?;
        if local_refs.is_empty() {
            return Ok(PushResult {
                objects_pushed: 0,
                refs_updated: 0,
            });
        }

        // Collect all local object IDs for negotiation.
        let local_ids: Vec<ObjectId> = store.list_objects()?.into_iter().collect();
        let want_refs: Vec<String> = local_refs.iter().map(|(name, _)| name.clone()).collect();

        // Negotiate: find which objects the remote needs.
        let negotiation = self.negotiate(&local_ids, &want_refs).await?;

        // Push needed objects.
        let mut objects_pushed = 0;
        for id_str in &negotiation.need {
            let id = parse_object_id(id_str)?;
            let obj = store.get(&id)?;
            self.put_object(&obj).await?;
            objects_pushed += 1;
        }

        // Update refs. Derive protocol and commit count from the commit object.
        let mut refs_updated = 0;
        for (name, id) in &local_refs {
            let remote_target = self.get_ref(name).await?;

            // Read the commit to get the protocol name and count ancestors.
            let (protocol, commit_count) = match store.get(id) {
                Ok(Object::Commit(c)) => {
                    let count = count_ancestors(store, id);
                    (c.protocol.clone(), count)
                }
                _ => ("project".to_owned(), 1),
            };

            self.set_ref(name, remote_target.as_ref(), id, &protocol, commit_count)
                .await?;
            refs_updated += 1;
        }

        Ok(PushResult {
            objects_pushed,
            refs_updated,
        })
    }

    /// Pull remote objects and refs into the local store.
    ///
    /// Flow: listRefs on remote, negotiate, getObject for each needed object,
    /// store locally, update local refs.
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError`] on any failure in the pull pipeline.
    pub async fn pull<S: Store>(&self, store: &mut S) -> Result<PullResult, XrpcError> {
        // List remote refs.
        let remote_refs = self.list_refs().await?;
        if remote_refs.is_empty() {
            return Ok(PullResult {
                objects_fetched: 0,
                refs_updated: 0,
            });
        }

        // Collect local object IDs for negotiation.
        let local_ids: Vec<ObjectId> = store.list_objects()?.into_iter().collect();
        let want_refs: Vec<String> = remote_refs.iter().map(|(name, _)| name.clone()).collect();

        // Negotiate: find which objects we need.
        let negotiation = self.negotiate(&local_ids, &want_refs).await?;

        // Fetch needed objects.
        let mut objects_fetched = 0;
        for id_str in &negotiation.need {
            let id = parse_object_id(id_str)?;
            let obj = self.get_object(&id).await?;
            store.put(&obj)?;
            objects_fetched += 1;
        }

        // Update local refs.
        let mut refs_updated = 0;
        for (name, id) in &remote_refs {
            store.set_ref(name, *id)?;
            refs_updated += 1;
        }

        Ok(PullResult {
            objects_fetched,
            refs_updated,
        })
    }
}

/// Result of a push operation.
#[derive(Debug)]
pub struct PushResult {
    /// Number of objects pushed to the remote.
    pub objects_pushed: usize,
    /// Number of refs updated on the remote.
    pub refs_updated: usize,
}

/// Result of a pull operation.
#[derive(Debug)]
pub struct PullResult {
    /// Number of objects fetched from the remote.
    pub objects_fetched: usize,
    /// Number of local refs updated.
    pub refs_updated: usize,
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Count the number of ancestors reachable from a commit (including itself).
fn count_ancestors<S: Store>(store: &S, start: &ObjectId) -> u64 {
    let mut count = 0;
    let mut stack = vec![*start];
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        count += 1;
        if let Ok(Object::Commit(c)) = store.get(&id) {
            stack.extend_from_slice(&c.parents);
        }
    }
    count
}

/// Build the URL for the `dev.panproto.node.listCommits` XRPC query.
///
/// Extracted as a pure helper so tests can verify query-parameter
/// composition without having to spin up an HTTP client.
fn build_list_commits_url(
    base_url: &str,
    did: &str,
    repo: &str,
    git_ref: Option<&str>,
    limit: Option<u32>,
) -> String {
    let mut url = format!("{base_url}/xrpc/dev.panproto.node.listCommits?did={did}&repo={repo}");
    if let Some(r) = git_ref {
        let _ = write!(url, "&ref={r}");
    }
    if let Some(n) = limit {
        let _ = write!(url, "&limit={n}");
    }
    url
}

/// Build the URL for the `dev.panproto.node.diffCommits` XRPC query.
fn build_diff_commits_url(
    base_url: &str,
    did: &str,
    repo: &str,
    from: &str,
    to: &str,
    context_lines: Option<u32>,
) -> String {
    let mut url = format!(
        "{base_url}/xrpc/dev.panproto.node.diffCommits?did={did}&repo={repo}&from={from}&to={to}"
    );
    if let Some(ctx) = context_lines {
        let _ = write!(url, "&contextLines={ctx}");
    }
    url
}

/// Parse a hex string into an `ObjectId`.
fn parse_object_id(hex: &str) -> Result<ObjectId, XrpcError> {
    let bytes =
        hex::decode(hex).map_err(|e| XrpcError::InvalidUrl(format!("bad object ID: {e}")))?;
    if bytes.len() != 32 {
        return Err(XrpcError::InvalidUrl(format!(
            "object ID must be 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(ObjectId::from_bytes(arr))
}

/// Check response status, consuming the response. Returns it on success, error with body on failure.
async fn check_response(
    resp: reqwest::Response,
    endpoint: &str,
) -> Result<reqwest::Response, XrpcError> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    Err(XrpcError::NodeError {
        endpoint: endpoint.to_owned(),
        status,
        body,
    })
}

/// Check response status, consuming the response to read the body on error.
async fn check_status_owned(
    resp: reqwest::Response,
    endpoint: &str,
) -> Result<ObjectId, XrpcError> {
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(XrpcError::NodeError {
            endpoint: endpoint.to_owned(),
            status,
            body,
        });
    }
    let body: serde_json::Value = resp.json().await?;
    let id_str = body["id"].as_str().ok_or_else(|| XrpcError::NodeError {
        endpoint: endpoint.to_owned(),
        status: 200,
        body: "missing id field in putObject response".to_owned(),
    })?;
    parse_object_id(id_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_commits_result_camel_case_roundtrip() -> Result<(), serde_json::Error> {
        let result = ListCommitsResult {
            commits: vec![CommitEntry {
                oid: "abc123".to_owned(),
                parents: vec!["def456".to_owned()],
                summary: "initial commit".to_owned(),
                message: "initial commit\n\nwith body".to_owned(),
                author: CommitIdentity {
                    name: "Alice".to_owned(),
                    email: Some("alice@example.com".to_owned()),
                },
                committer: None,
                timestamp: 1_712_345_678,
                tree_oid: Some("fff000".to_owned()),
            }],
            count: 1,
            start: Some("abc123".to_owned()),
        };
        let json = serde_json::to_value(&result)?;
        assert!(json["commits"][0]["treeOid"].is_string());
        assert!(json["commits"][0]["tree_oid"].is_null());
        let roundtrip: ListCommitsResult = serde_json::from_value(json)?;
        assert_eq!(roundtrip.commits[0].oid, "abc123");
        assert_eq!(roundtrip.commits[0].tree_oid.as_deref(), Some("fff000"));
        Ok(())
    }

    #[test]
    fn diff_commits_result_camel_case_roundtrip() -> Result<(), serde_json::Error> {
        let result = DiffCommitsResult {
            from: "aaa".to_owned(),
            to: "bbb".to_owned(),
            files: vec![FileDiff {
                path: "schemas/core.json".to_owned(),
                old_path: None,
                status: "added".to_owned(),
                old_oid: None,
                new_oid: Some("ccc".to_owned()),
                additions: 12,
                deletions: 0,
                binary: false,
                hunks: vec![],
                structural_diff: Some(serde_json::json!({"added_vertices": ["Foo"]})),
            }],
            total_additions: 12,
            total_deletions: 0,
            file_count: 1,
        };
        let json = serde_json::to_value(&result)?;
        assert!(json["totalAdditions"].is_number());
        assert!(json["total_additions"].is_null());
        assert!(json["files"][0]["oldPath"].is_null());
        assert!(json["files"][0]["structuralDiff"].is_object());
        let roundtrip: DiffCommitsResult = serde_json::from_value(json)?;
        assert_eq!(roundtrip.total_additions, 12);
        assert_eq!(roundtrip.files[0].path, "schemas/core.json");
        Ok(())
    }

    // ── URL builder tests ──────────────────────────────────────────────

    #[test]
    fn list_commits_url_minimal_required_params_only() {
        let url = build_list_commits_url(
            "https://node.example.com",
            "did:plc:abc",
            "myrepo",
            None,
            None,
        );
        assert_eq!(
            url,
            "https://node.example.com/xrpc/dev.panproto.node.listCommits?did=did:plc:abc&repo=myrepo"
        );
    }

    #[test]
    fn list_commits_url_with_ref_only() {
        let url = build_list_commits_url(
            "https://node.example.com",
            "did:plc:abc",
            "myrepo",
            Some("refs/heads/main"),
            None,
        );
        assert!(url.ends_with("&ref=refs/heads/main"));
        assert!(!url.contains("&limit="));
    }

    #[test]
    fn list_commits_url_with_limit_only() {
        let url = build_list_commits_url(
            "https://node.example.com",
            "did:plc:abc",
            "myrepo",
            None,
            Some(100),
        );
        assert!(url.ends_with("&limit=100"));
        assert!(!url.contains("&ref="));
    }

    #[test]
    fn list_commits_url_with_ref_and_limit() {
        let url = build_list_commits_url(
            "https://node.example.com",
            "did:plc:abc",
            "myrepo",
            Some("feature"),
            Some(25),
        );
        assert_eq!(
            url,
            "https://node.example.com/xrpc/dev.panproto.node.listCommits?did=did:plc:abc&repo=myrepo&ref=feature&limit=25"
        );
    }

    #[test]
    fn diff_commits_url_without_context_lines() {
        let url = build_diff_commits_url(
            "https://node.example.com",
            "did:plc:abc",
            "myrepo",
            "deadbeef",
            "cafef00d",
            None,
        );
        assert_eq!(
            url,
            "https://node.example.com/xrpc/dev.panproto.node.diffCommits?did=did:plc:abc&repo=myrepo&from=deadbeef&to=cafef00d"
        );
    }

    #[test]
    fn diff_commits_url_with_context_lines() {
        let url = build_diff_commits_url(
            "https://node.example.com",
            "did:plc:abc",
            "myrepo",
            "deadbeef",
            "cafef00d",
            Some(5),
        );
        assert!(url.ends_with("&contextLines=5"));
    }

    #[test]
    fn list_commits_url_strips_trailing_slash_from_base() {
        // NodeClient::new already trims trailing slashes from base_url,
        // so the builder should receive a canonical base. This test
        // exercises the documented precondition.
        let url =
            build_list_commits_url("https://node.example.com", "did:plc:abc", "r", None, None);
        // Two slashes in a row would indicate a bug: it should be
        // "https://node.example.com/xrpc/..." not ".com//xrpc...".
        let Some(after_scheme) = url.strip_prefix("https://") else {
            panic!("url should start with https://: {url}");
        };
        assert!(
            !after_scheme.contains("//"),
            "url should not contain consecutive slashes: {url}"
        );
    }
}
