//! XRPC client for cospan node VCS operations.
//!
//! Implements the `dev.cospan.node.*` XRPC endpoints for push/pull/clone
//! of panproto-vcs objects between local stores and remote cospan nodes.

use panproto_vcs::{HeadState, Object, ObjectId, Store};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::XrpcError;

/// A client for communicating with a cospan node's XRPC endpoints.
#[derive(Debug, Clone)]
pub struct NodeClient {
    /// Base URL of the cospan node (e.g. `https://node.cospan.dev`).
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

impl NodeClient {
    /// Create a new client for a cospan node.
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

    /// Parse a `cospan://did/repo` URL into (base_url, did, repo).
    ///
    /// The base URL defaults to `https://node.cospan.dev` unless overridden
    /// by the `COSPAN_NODE_URL` environment variable.
    ///
    /// # Errors
    ///
    /// Returns [`XrpcError::InvalidUrl`] if the URL format is invalid.
    pub fn from_url(url: &str) -> Result<Self, XrpcError> {
        let path = url
            .strip_prefix("cospan://")
            .ok_or_else(|| XrpcError::InvalidUrl(format!("expected cospan:// prefix: {url}")))?;

        let parts: Vec<&str> = path.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(XrpcError::InvalidUrl(format!(
                "expected cospan://did/repo: {url}"
            )));
        }

        let base = std::env::var("COSPAN_NODE_URL")
            .unwrap_or_else(|_| "https://node.cospan.dev".to_owned());

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
            "{}/xrpc/dev.cospan.node.getObject?did={}&repo={}&id={}",
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
            "{}/xrpc/dev.cospan.node.getRef?did={}&repo={}&ref={}",
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
            "{}/xrpc/dev.cospan.node.listRefs?did={}&repo={}",
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
            "{}/xrpc/dev.cospan.node.getHead?did={}&repo={}",
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
            "{}/xrpc/dev.cospan.node.getRepoInfo?did={}&repo={}",
            self.base_url, self.did, self.repo
        );
        let resp = self.http.get(&url).send().await?;
        let resp = check_response(resp, "getRepoInfo").await?;
        let info: RepoInfo = resp.json().await?;
        Ok(info)
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
            "{}/xrpc/dev.cospan.node.putObject?did={}&repo={}",
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

        let url = format!("{}/xrpc/dev.cospan.node.setRef", self.base_url);
        let body = serde_json::json!({
            "did": self.did,
            "repo": self.repo,
            "ref": ref_name,
            "oldTarget": old_target.map(|id| id.to_string()),
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
        let url = format!("{}/xrpc/dev.cospan.node.negotiate", self.base_url);
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
