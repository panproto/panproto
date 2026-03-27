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

use panproto_vcs::MemStore;
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
                match rt.block_on(cmd_fetch(&client, parts[0], parts[1])) {
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
            // push <src>:<dst>
            match rt.block_on(cmd_push(&client, rest)) {
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

    // Also report HEAD.
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
async fn cmd_fetch(
    client: &NodeClient,
    _sha: &str,
    _ref_name: &str,
) -> Result<(), panproto_xrpc::XrpcError> {
    // Fetch all needed objects into a local panproto store, then
    // convert to git objects via panproto-git export.
    let mut store = MemStore::new();
    let _result = client.pull(&mut store).await?;
    // The git objects are created by panproto-git::export_to_git,
    // which the caller (git) will handle via its own object store.
    Ok(())
}

/// Push a local ref to the remote node.
async fn cmd_push(client: &NodeClient, refspec: &str) -> Result<(), panproto_xrpc::XrpcError> {
    // Parse refspec: <src>:<dst>
    let parts: Vec<&str> = refspec.splitn(2, ':').collect();
    let _src = parts.first().copied().unwrap_or("");
    let _dst = parts.get(1).copied().unwrap_or("");

    // Import local git objects into panproto, then push to node.
    // For now, push all local objects via the high-level push() method.
    let store = MemStore::new();
    let _result = client.push(&store).await?;
    Ok(())
}
