//! Git remote helper for `cospan://` URLs.
//!
//! Git calls this binary as `git-remote-cospan` when encountering a remote URL
//! starting with `cospan://`. Communication happens via stdin/stdout using the
//! git remote-helper protocol.
//!
//! ## Protocol
//!
//! Git sends commands on stdin, one per line:
//!
//! - `capabilities` — respond with supported capabilities
//! - `list` — list refs on the remote
//! - `list for-push` — list refs (for push context)
//! - `fetch <sha> <ref>` — fetch objects for the given ref
//! - `push <src>:<dst>` — push a local ref to the remote
//! - (empty line) — end of batch
//!
//! ## Usage
//!
//! ```sh
//! git clone cospan://did:plc:abc123/my-repo
//! git push cospan main
//! git pull cospan main
//! ```

use std::io::{self, BufRead, Write};

use panproto_vcs::{MemStore, Store};
use panproto_xrpc::NodeClient;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Git calls: git-remote-cospan <remote-name> <url>
    if args.len() < 3 {
        eprintln!("usage: git-remote-cospan <remote> <url>");
        std::process::exit(1);
    }

    let url = &args[2];
    let client = match NodeClient::from_url(url) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    // Apply auth token from environment.
    let client = match std::env::var("COSPAN_TOKEN") {
        Ok(token) => client.with_token(&token),
        Err(_) => client,
    };

    let rt = tokio::runtime::Runtime::new().unwrap_or_else(|e| {
        eprintln!("error creating tokio runtime: {e}");
        std::process::exit(1);
    });

    // Open the local git repo (git sets GIT_DIR before calling the remote helper).
    let git_dir = std::env::var("GIT_DIR").unwrap_or_else(|_| ".git".to_owned());
    let local_git_repo = match git2::Repository::open(&git_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error opening local git repo at {git_dir}: {e}");
            std::process::exit(1);
        }
    };

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("error reading stdin: {e}");
                break;
            }
        };

        let line = line.trim();

        if line.is_empty() {
            // End of batch. Flush and continue.
            let _ = writeln!(out);
            let _ = out.flush();
            continue;
        }

        if line == "capabilities" {
            let _ = writeln!(out, "fetch");
            let _ = writeln!(out, "push");
            let _ = writeln!(out);
            let _ = out.flush();
            continue;
        }

        if line == "list" || line == "list for-push" {
            match rt.block_on(cmd_list(&client)) {
                Ok(refs) => {
                    for (id, name) in &refs {
                        let _ = writeln!(out, "{id} {name}");
                    }
                    let _ = writeln!(out);
                    let _ = out.flush();
                }
                Err(e) => {
                    eprintln!("error listing refs: {e}");
                    break;
                }
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("fetch ") {
            // fetch <sha> <ref>
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if parts.len() == 2 {
                match rt.block_on(cmd_fetch(&client, parts[1], &local_git_repo)) {
                    Ok(()) => {}
                    Err(e) => {
                        eprintln!("error fetching {}: {e}", parts[1]);
                    }
                }
            }
            // Fetch commands are batched; git sends an empty line when done.
            continue;
        }

        if let Some(rest) = line.strip_prefix("push ") {
            match rt.block_on(cmd_push(&client, rest, &local_git_repo)) {
                Ok(()) => {
                    let _ = writeln!(out, "ok {rest}");
                }
                Err(e) => {
                    let _ = writeln!(out, "error {rest} {e}");
                }
            }
            continue;
        }

        // Unknown command.
        eprintln!("git-remote-cospan: unknown command: {line}");
    }
}

/// List refs on the remote node.
async fn cmd_list(client: &NodeClient) -> Result<Vec<(String, String)>, panproto_xrpc::XrpcError> {
    let refs = client.list_refs().await?;
    let mut result: Vec<(String, String)> = Vec::new();

    for (name, id) in refs {
        result.push((id.to_string(), name));
    }

    // Report HEAD.
    let head = client.get_head().await?;
    match head {
        panproto_vcs::HeadState::Branch(branch) => {
            result.push((format!("@refs/heads/{branch}"), "HEAD".to_owned()));
        }
        panproto_vcs::HeadState::Detached(id) => {
            result.push((id.to_string(), "HEAD".to_owned()));
        }
    }

    Ok(result)
}

/// Fetch objects for a ref from the remote node into the local git repo.
///
/// Pulls the ref's objects from the remote panproto node into a temporary
/// panproto store, then converts each panproto commit to a git commit via
/// `panproto-git::export_to_git` and writes it into the local git repo.
async fn cmd_fetch(
    client: &NodeClient,
    ref_name: &str,
    git_repo: &git2::Repository,
) -> Result<(), Box<dyn std::error::Error>> {
    // Pull the remote ref and all reachable objects into a local panproto store.
    let mut store = MemStore::new();
    client.pull(&mut store).await?;

    // Find the commit ID for the requested ref.
    let ref_id = store
        .get_ref(ref_name)?
        .ok_or_else(|| format!("ref {ref_name} not found after pull"))?;

    // Export the panproto commit as a git commit in the local repo.
    let parent_map = rustc_hash::FxHashMap::default();
    panproto_git::export_to_git(&store, git_repo, ref_id, &parent_map)?;

    Ok(())
}

/// Push a local ref to the remote node.
///
/// Reads the git commit for the source ref from the local git repo,
/// imports it into a temporary panproto store via `panproto-git::import_git_repo`,
/// and pushes the resulting objects to the remote node.
async fn cmd_push(
    client: &NodeClient,
    refspec: &str,
    git_repo: &git2::Repository,
) -> Result<(), Box<dyn std::error::Error>> {
    // Parse refspec: <src>:<dst>
    let parts: Vec<&str> = refspec.splitn(2, ':').collect();
    let src = parts.first().copied().unwrap_or("HEAD");
    let dst = parts.get(1).copied().unwrap_or(src);

    // Import the local git ref into a panproto store.
    let mut store = MemStore::new();
    let import_result = panproto_git::import_git_repo(git_repo, &mut store, src)?;

    // Push all imported objects to the remote node.
    client.push(&store).await?;

    // Update the remote ref to point to the imported HEAD.
    let remote_target = client.get_ref(dst).await?;
    client
        .set_ref(
            dst,
            remote_target.as_ref(),
            &import_result.head_id,
            "project",
            u64::try_from(import_result.commit_count).unwrap_or(0),
        )
        .await?;

    Ok(())
}
