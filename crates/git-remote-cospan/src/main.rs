#![allow(clippy::future_not_send)]
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
//! - `capabilities`: respond with supported capabilities
//! - `list`: list refs on the remote
//! - `list for-push`: list refs (for push context)
//! - `fetch <sha> <ref>`: fetch objects for the given ref
//! - `push <src>:<dst>`: push a local ref to the remote
//! - (empty line): end of batch
//!
//! ## Usage
//!
//! ```sh
//! git clone cospan://did:plc:abc123/my-repo
//! git push cospan main
//! git pull cospan main
//! ```

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use panproto_vcs::{FsStore, ObjectId, Store};
use panproto_xrpc::NodeClient;
use rustc_hash::FxHashMap;

/// Abstraction over the subset of `NodeClient` operations used by
/// `cmd_push` and `cmd_fetch`. Exists so tests can substitute an
/// in-process fake in place of a real HTTP client.
///
/// Using a trait here (rather than calling `NodeClient` directly) lets
/// us exercise the full pipeline of both subcommands without having to
/// spin up an HTTP server.
#[allow(async_fn_in_trait)]
trait RemoteClient {
    /// Pull all remote objects and refs into `store`.
    async fn remote_pull(&self, store: &mut FsStore) -> Result<(), Box<dyn std::error::Error>>;

    /// Push all objects and refs in `store` to the remote.
    async fn remote_push(&self, store: &FsStore) -> Result<(), Box<dyn std::error::Error>>;

    /// Resolve a named ref on the remote to its current target, if any.
    async fn remote_get_ref(
        &self,
        ref_name: &str,
    ) -> Result<Option<ObjectId>, Box<dyn std::error::Error>>;

    /// Update a named ref on the remote. `old_target` is used for
    /// compare-and-swap semantics on the server.
    async fn remote_set_ref(
        &self,
        ref_name: &str,
        old_target: Option<&ObjectId>,
        new_target: &ObjectId,
        protocol: &str,
        commit_count: u64,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

impl RemoteClient for NodeClient {
    async fn remote_pull(&self, store: &mut FsStore) -> Result<(), Box<dyn std::error::Error>> {
        self.pull(store).await?;
        Ok(())
    }

    async fn remote_push(&self, store: &FsStore) -> Result<(), Box<dyn std::error::Error>> {
        self.push(store).await?;
        Ok(())
    }

    async fn remote_get_ref(
        &self,
        ref_name: &str,
    ) -> Result<Option<ObjectId>, Box<dyn std::error::Error>> {
        Ok(self.get_ref(ref_name).await?)
    }

    async fn remote_set_ref(
        &self,
        ref_name: &str,
        old_target: Option<&ObjectId>,
        new_target: &ObjectId,
        protocol: &str,
        commit_count: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.set_ref(ref_name, old_target, new_target, protocol, commit_count)
            .await?;
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Git calls: git-remote-cospan <remote-name> <url>
    if args.len() < 3 {
        eprintln!("usage: git-remote-cospan <remote> <url>");
        std::process::exit(1);
    }

    let remote_name = &args[1];
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

    // Per-remote persistent cache. Holds a panproto-vcs FsStore with the
    // imported objects plus a git↔panproto marks file. Enables incremental
    // imports across pushes: on every `git push`, only new commits are
    // translated into panproto objects.
    let cache_dir = Path::new(&git_dir).join("cospan-cache").join(remote_name);

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
                match rt.block_on(cmd_fetch(&client, parts[1], &local_git_repo, &cache_dir)) {
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
            let dst = push_refspec_dst(rest);
            match rt.block_on(cmd_push(&client, rest, &local_git_repo, &cache_dir)) {
                Ok(()) => {
                    let _ = writeln!(out, "ok {dst}");
                }
                Err(e) => {
                    eprintln!("git-remote-cospan: push {dst} failed: {e}");
                    let _ = writeln!(out, "error {dst} {e}");
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
/// Pulls the ref's objects into the persistent panproto cache (so that
/// negotiate on the next pull/push can skip what we already have), then
/// walks the panproto commit DAG in parent-first order and exports any
/// commit we haven't already exported to git. The marks file is used
/// both to seed the panproto→git parent lookup and to record newly
/// translated commits so that subsequent fetches stay incremental.
async fn cmd_fetch<C: RemoteClient>(
    client: &C,
    ref_name: &str,
    git_repo: &git2::Repository,
    cache_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Open (or initialize) the per-remote persistent cache.
    let mut store = open_or_init_cache(cache_dir)?;

    // Pull the remote ref's objects into the cache. Negotiate will skip
    // anything we already have locally.
    client.remote_pull(&mut store).await?;

    // The rest of the work is pure local state manipulation; delegate to
    // a testable helper.
    let report = fetch_export_stage(&store, git_repo, cache_dir, ref_name)?;
    if report.commits_exported > 0 {
        eprintln!(
            "git-remote-cospan: fetched {}/{} new commits for {ref_name}",
            report.commits_exported, report.commits_walked
        );
    }
    if let Some(oid) = report.tip_git_oid {
        eprintln!("git-remote-cospan: {ref_name} tip = {oid}");
    }

    Ok(())
}

/// Result of a single fetch-export stage, returned for test inspection.
#[derive(Debug, Default)]
struct FetchExportReport {
    /// Number of panproto commits that were newly exported as git commits.
    commits_exported: usize,
    /// Total number of commits reachable from the tip (including skipped).
    commits_walked: usize,
    /// Git OID of the final exported commit (or the already-known OID of
    /// the tip if nothing new was exported).
    tip_git_oid: Option<git2::Oid>,
}

/// Walk the panproto DAG from `ref_name` and export any commits that
/// aren't already in the marks file. Pure local: assumes the store is
/// already populated with the objects reachable from the tip.
///
/// The walk is a **topological** iterative DFS post-order from the tip,
/// which guarantees that every parent appears before its children in
/// the export order. This is required for the parent-lookup in
/// `export_to_git` to succeed via the growing `panproto_to_git` map:
/// if parents were emitted after children, the child's
/// `export_to_git` call would find its parents unmapped and silently
/// drop them, disconnecting the exported git DAG.
///
/// We deliberately avoid `panproto_vcs::dag::log_walk` here because it
/// orders by commit timestamp (max-heap) rather than by DAG structure.
/// Git commits can have non-monotonic author timestamps (rebases,
/// amends, `--date` overrides), so a chronological walk is not a valid
/// topological sort in general.
fn fetch_export_stage<S: Store>(
    store: &S,
    git_repo: &git2::Repository,
    cache_dir: &Path,
    ref_name: &str,
) -> Result<FetchExportReport, Box<dyn std::error::Error>> {
    let tip_id = store
        .get_ref(ref_name)?
        .ok_or_else(|| format!("ref {ref_name} not found in store"))?;

    // Build a panproto→git parent map seeded from the existing marks.
    // Any panproto commit whose git OID is already recorded is skipped
    // during the DAG walk; when we export a new commit whose parents
    // live on the "already-exported" side of the cut, we look them up
    // here to preserve DAG structure in git.
    //
    // We filter out stale marks whose git OID no longer exists in the
    // destination repo (e.g., because the user ran `git gc` or
    // `git reflog expire`). Leaving stale entries would cause
    // `export_to_git` to pass a dead git OID as a parent to
    // `git2::Repository::commit`, which silently drops it and
    // disconnects the exported DAG. Dropping the stale mark instead
    // lets the topological walk re-export the affected commit
    // naturally, restoring a well-formed DAG.
    let marks_path = marks_path(cache_dir);
    let git_marks = load_marks(&marks_path);
    let mut panproto_to_git: FxHashMap<ObjectId, git2::Oid> = git_marks
        .iter()
        .filter_map(|(g, p)| {
            if git_repo.find_commit(*g).is_ok() {
                Some((*p, *g))
            } else {
                None
            }
        })
        .collect();

    let topo_order = topo_walk_from(store, tip_id)?;
    let commits_walked = topo_order.len();

    let mut new_marks: Vec<(git2::Oid, ObjectId)> = Vec::new();
    let mut last_exported: Option<git2::Oid> = None;
    for (panproto_id, _) in &topo_order {
        if panproto_to_git.contains_key(panproto_id) {
            continue;
        }
        let result =
            panproto_git::export_to_git(store, git_repo, *panproto_id, &panproto_to_git, None)?;
        panproto_to_git.insert(*panproto_id, result.git_oid);
        new_marks.push((result.git_oid, *panproto_id));
        last_exported = Some(result.git_oid);
    }

    // Persist any new mappings so the next fetch is also incremental.
    if !new_marks.is_empty() {
        append_marks(&marks_path, &new_marks)?;
    }

    // Report the git OID the caller should use when updating its refs.
    // Prefer the freshly-exported tip; otherwise fall back to the git OID
    // already recorded for the tip's panproto id.
    let tip_git_oid = last_exported.or_else(|| panproto_to_git.get(&tip_id).copied());

    Ok(FetchExportReport {
        commits_exported: new_marks.len(),
        commits_walked,
        tip_git_oid,
    })
}

/// Each stack frame for the iterative DFS in `topo_walk_from`. On
/// first visit (`Enter`) we schedule the node's parents for visiting,
/// then on second visit (`Emit`, popped after all parents have been
/// emitted) we append the node to the result. The `Emit` variant
/// carries the already-loaded commit to avoid a redundant store read.
enum TopoFrame {
    Enter(ObjectId),
    Emit(ObjectId, Box<panproto_vcs::CommitObject>),
}

/// Load a commit from the panproto store, erroring if the stored
/// object isn't actually a commit.
fn load_commit<S: Store>(
    store: &S,
    id: ObjectId,
) -> Result<panproto_vcs::CommitObject, Box<dyn std::error::Error>> {
    match store.get(&id)? {
        panproto_vcs::Object::Commit(c) => Ok(c),
        other => Err(format!(
            "topo_walk: expected commit at {id}, got {}",
            other.type_name()
        )
        .into()),
    }
}

/// Produce a topological ordering of the panproto commit DAG reachable
/// from `tip`, parents first. Uses an iterative DFS post-order: when
/// we finish visiting all of a node's parents, we emit the node.
///
/// Iterative (not recursive) to avoid blowing the Rust stack on deep
/// histories. Returns `(ObjectId, CommitObject)` pairs so the caller
/// does not have to re-fetch commit objects to hash or read them.
fn topo_walk_from<S: Store>(
    store: &S,
    tip: ObjectId,
) -> Result<Vec<(ObjectId, panproto_vcs::CommitObject)>, Box<dyn std::error::Error>> {
    use std::collections::HashSet;

    let mut result: Vec<(ObjectId, panproto_vcs::CommitObject)> = Vec::new();
    // `visited` tracks nodes we've started visiting (to avoid re-entering).
    let mut visited: HashSet<ObjectId> = HashSet::default();
    // `emitted` tracks nodes that have been pushed to `result`, so we
    // can skip them cleanly if they show up again via another path.
    let mut emitted: HashSet<ObjectId> = HashSet::default();
    let mut stack: Vec<TopoFrame> = vec![TopoFrame::Enter(tip)];

    while let Some(frame) = stack.pop() {
        match frame {
            TopoFrame::Enter(id) => {
                if !visited.insert(id) {
                    // Already entered via another path; nothing new to
                    // schedule. The existing Emit frame (if any) will
                    // eventually emit this node.
                    continue;
                }
                let commit = load_commit(store, id)?;
                // Snapshot the parent list before moving commit into
                // the Emit frame. This avoids cloning the entire
                // CommitObject (which carries variable-length message
                // and author strings).
                let parents: Vec<ObjectId> = commit.parents.clone();
                // Schedule Emit first (carrying the commit so we don't
                // re-read it from the store), then schedule Enter for
                // each parent (in reverse so that the first parent is
                // processed first, matching git's convention for merge
                // commits).
                stack.push(TopoFrame::Emit(id, Box::new(commit)));
                for parent in parents.iter().rev() {
                    if !visited.contains(parent) {
                        stack.push(TopoFrame::Enter(*parent));
                    }
                }
            }
            TopoFrame::Emit(id, commit) => {
                if !emitted.insert(id) {
                    continue;
                }
                result.push((id, *commit));
            }
        }
    }

    Ok(result)
}

/// Push a local ref to the remote node.
///
/// Reads the git commit for the source ref from the local git repo,
/// incrementally imports any new commits into a persistent panproto cache
/// (under `$GIT_DIR/cospan-cache/<remote>/`), and pushes the resulting
/// objects to the remote node. The cache persists across invocations so
/// that subsequent pushes only translate the commits that are actually new
/// since the previous push.
async fn cmd_push<C: RemoteClient>(
    client: &C,
    refspec: &str,
    git_repo: &git2::Repository,
    cache_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Parse refspec: <src>:<dst>
    let parts: Vec<&str> = refspec.splitn(2, ':').collect();
    let src = parts.first().copied().unwrap_or("HEAD");
    let dst = parts.get(1).copied().unwrap_or(src);

    // Open (or initialize) the persistent FsStore for this remote.
    let mut store = open_or_init_cache(cache_dir)?;

    // Run the pure local stage: incremental import, marks update, ref
    // write on the local store.
    let report = push_import_stage(&mut store, git_repo, cache_dir, src, dst)?;
    if report.new_commits > 0 {
        eprintln!(
            "git-remote-cospan: imported {} new commits ({} total)",
            report.new_commits, report.total_commits
        );
    }

    // Push accumulated objects to the remote (negotiate skips what the
    // remote already has, so this stays cheap even with a growing cache).
    client.remote_push(&store).await?;

    // Update the remote ref. The total commit count for metadata is the
    // full history depth, not just what was imported this round.
    let remote_target = client.remote_get_ref(dst).await?;
    client
        .remote_set_ref(
            dst,
            remote_target.as_ref(),
            &report.head_id,
            "project",
            u64::try_from(report.total_commits).unwrap_or(0),
        )
        .await?;

    Ok(())
}

/// Result of a single push-import stage, returned for test inspection.
#[derive(Debug)]
struct PushImportReport {
    /// Panproto commit ID of the imported tip.
    head_id: ObjectId,
    /// Number of commits newly imported on this call.
    new_commits: usize,
    /// Total number of commits known in the persistent cache after this
    /// call (= previous marks + new imports).
    total_commits: usize,
}

/// Incrementally import `src` from `git_repo` into `store`, persisting
/// new git↔panproto mappings to the marks file at `cache_dir` and
/// setting the local panproto `dst` ref to the imported tip.
///
/// Pure local: does not perform any network I/O. Splitting this out of
/// `cmd_push` lets integration tests exercise the incremental pipeline
/// without needing a mock `NodeClient`.
fn push_import_stage(
    store: &mut FsStore,
    git_repo: &git2::Repository,
    cache_dir: &Path,
    src: &str,
    dst: &str,
) -> Result<PushImportReport, Box<dyn std::error::Error>> {
    let marks_path = marks_path(cache_dir);
    let known = load_marks(&marks_path);
    let previously_known = known.len();

    // Incrementally import: only git commits whose OID is not already in
    // `known` are walked, parsed, and stored as panproto objects.
    let import_result = panproto_git::import_git_repo_incremental(git_repo, store, src, &known)?;

    // Persist the new mappings so the next push is also incremental.
    if !import_result.oid_map.is_empty() {
        append_marks(&marks_path, &import_result.oid_map)?;
    }

    // Update the local panproto ref to name the imported tip. `client.push`
    // iterates `refs/` in the local store and mirrors each ref to the remote,
    // so this is what tells the push pipeline which branch to publish.
    if import_result.head_id != ObjectId::ZERO {
        store.set_ref(dst, import_result.head_id)?;
    }

    Ok(PushImportReport {
        head_id: import_result.head_id,
        new_commits: import_result.commit_count,
        total_commits: previously_known + import_result.commit_count,
    })
}

/// Extract the destination ref from a `push` refspec (`<src>:<dst>`).
///
/// Git's remote-helper protocol requires `ok <dst>` / `error <dst> <why>`
/// responses. Reporting the full refspec instead leaves git unable to
/// match the status line to a push entry, so it silently reports
/// "Everything up-to-date" even when the push failed or no-oped.
fn push_refspec_dst(refspec: &str) -> &str {
    refspec
        .splitn(2, ':')
        .nth(1)
        .unwrap_or(refspec)
        .trim_start_matches('+')
        .trim()
}

/// Path of the git↔panproto marks file for a given cache directory.
fn marks_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("git-marks.txt")
}

/// Open the persistent panproto cache at `cache_dir`, initializing it if
/// it does not yet exist.
fn open_or_init_cache(cache_dir: &Path) -> Result<FsStore, Box<dyn std::error::Error>> {
    if cache_dir.join(".panproto").is_dir() {
        Ok(FsStore::open(cache_dir)?)
    } else {
        std::fs::create_dir_all(cache_dir)?;
        Ok(FsStore::init(cache_dir)?)
    }
}

/// Load a git↔panproto mapping from a plain-text marks file.
///
/// File format: one entry per line, `<git_oid_hex> <panproto_oid_hex>`.
/// Missing or malformed files yield an empty map (the next push will
/// simply act as a full import).
fn load_marks(marks_path: &Path) -> FxHashMap<git2::Oid, ObjectId> {
    let mut map = FxHashMap::default();
    let Ok(content) = std::fs::read_to_string(marks_path) else {
        return map;
    };
    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let Some(git_hex) = parts.next() else {
            continue;
        };
        let Some(panproto_hex) = parts.next() else {
            continue;
        };
        let Ok(git_oid) = git2::Oid::from_str(git_hex) else {
            continue;
        };
        let Ok(panproto_id) = panproto_hex.parse::<ObjectId>() else {
            continue;
        };
        map.insert(git_oid, panproto_id);
    }
    map
}

/// Append new git↔panproto mapping entries to the marks file.
fn append_marks(
    marks_path: &Path,
    entries: &[(git2::Oid, ObjectId)],
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = marks_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(marks_path)?;
    for (git_oid, panproto_id) in entries {
        writeln!(file, "{git_oid} {panproto_id}")?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::Path;

    use super::{
        RemoteClient, append_marks, cmd_fetch, cmd_push, fetch_export_stage, load_marks,
        marks_path, open_or_init_cache, push_import_stage, push_refspec_dst,
    };
    use panproto_vcs::{FsStore, MemStore, Object, ObjectId, Store};
    use std::cell::RefCell;

    /// A deterministic 32-byte panproto `ObjectId` built from a single seed byte.
    fn fake_panproto_id(seed: u8) -> ObjectId {
        ObjectId::from_bytes([seed; 32])
    }

    /// A deterministic git OID built from a single seed nibble (repeated).
    fn fake_git_oid(hex_char: char) -> git2::Oid {
        let s: String = std::iter::repeat_n(hex_char, 40).collect();
        git2::Oid::from_str(&s).unwrap()
    }

    #[test]
    fn push_refspec_dst_extracts_destination() {
        assert_eq!(
            push_refspec_dst("refs/heads/main:refs/heads/main"),
            "refs/heads/main"
        );
        assert_eq!(
            push_refspec_dst("refs/heads/feature:refs/heads/main"),
            "refs/heads/main"
        );
        // Force-push prefix.
        assert_eq!(
            push_refspec_dst("+refs/heads/main:refs/heads/main"),
            "refs/heads/main"
        );
        // Deletion refspec (empty src).
        assert_eq!(push_refspec_dst(":refs/heads/gone"), "refs/heads/gone");
        // Malformed input (no colon) falls back to the whole token.
        assert_eq!(push_refspec_dst("refs/heads/main"), "refs/heads/main");
    }

    #[test]
    fn marks_path_is_next_to_cache_dir() {
        let p = marks_path(std::path::Path::new("/tmp/cache"));
        assert_eq!(p, std::path::PathBuf::from("/tmp/cache/git-marks.txt"));
    }

    #[test]
    fn load_marks_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.txt");
        let result = load_marks(&path);
        assert!(result.is_empty());
    }

    #[test]
    fn load_marks_parses_well_formed_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("marks.txt");

        let git_a = fake_git_oid('a');
        let git_b = fake_git_oid('b');
        let pan_a = fake_panproto_id(0x11);
        let pan_b = fake_panproto_id(0x22);

        let content = format!("{git_a} {pan_a}\n{git_b} {pan_b}\n");
        std::fs::write(&path, content).unwrap();

        let marks = load_marks(&path);
        assert_eq!(marks.len(), 2);
        assert_eq!(marks.get(&git_a).copied(), Some(pan_a));
        assert_eq!(marks.get(&git_b).copied(), Some(pan_b));
    }

    #[test]
    fn load_marks_skips_malformed_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("marks.txt");

        let git_a = fake_git_oid('a');
        let pan_a = fake_panproto_id(0x11);

        // Mix of: blank, too-few fields, invalid git OID, invalid panproto,
        // and one well-formed line.
        let content = format!(
            "\n\
             onlyonefield\n\
             zz {pan_a}\n\
             {git_a} not_a_hash\n\
             {git_a} {pan_a}\n"
        );
        std::fs::write(&path, content).unwrap();

        let marks = load_marks(&path);
        assert_eq!(marks.len(), 1);
        assert_eq!(marks.get(&git_a).copied(), Some(pan_a));
    }

    #[test]
    fn append_marks_creates_file_and_round_trips_via_load() {
        let dir = tempfile::tempdir().unwrap();
        // Nested path: append_marks should create parent directories.
        let path = dir.path().join("nested/subdir/marks.txt");

        let git_a = fake_git_oid('a');
        let pan_a = fake_panproto_id(0x33);

        append_marks(&path, &[(git_a, pan_a)]).unwrap();
        assert!(path.exists());

        let marks = load_marks(&path);
        assert_eq!(marks.len(), 1);
        assert_eq!(marks.get(&git_a).copied(), Some(pan_a));
    }

    #[test]
    fn append_marks_appends_to_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("marks.txt");

        let git_a = fake_git_oid('a');
        let git_b = fake_git_oid('b');
        let pan_a = fake_panproto_id(0x11);
        let pan_b = fake_panproto_id(0x22);

        append_marks(&path, &[(git_a, pan_a)]).unwrap();
        append_marks(&path, &[(git_b, pan_b)]).unwrap();

        let marks = load_marks(&path);
        assert_eq!(marks.len(), 2);
        assert_eq!(marks.get(&git_a).copied(), Some(pan_a));
        assert_eq!(marks.get(&git_b).copied(), Some(pan_b));
    }

    #[test]
    fn append_marks_duplicate_entries_keeps_latest() {
        // The marks file is append-only, so a repeated git OID appears
        // twice in the file. `load_marks` should let the last entry win.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("marks.txt");

        let git_a = fake_git_oid('a');
        let pan_old = fake_panproto_id(0x11);
        let pan_new = fake_panproto_id(0x22);

        append_marks(&path, &[(git_a, pan_old)]).unwrap();
        append_marks(&path, &[(git_a, pan_new)]).unwrap();

        let marks = load_marks(&path);
        assert_eq!(marks.len(), 1);
        assert_eq!(
            marks.get(&git_a).copied(),
            Some(pan_new),
            "later entry should override earlier one"
        );
    }

    #[test]
    fn open_or_init_cache_creates_fresh_store() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        assert!(!cache.join(".panproto").exists());

        let mut store = open_or_init_cache(&cache).unwrap();
        assert!(cache.join(".panproto").is_dir());

        // Should behave like a real store: we can round-trip a ref write.
        let id = fake_panproto_id(0x77);
        store.set_ref("refs/heads/test", id).unwrap();
        assert_eq!(store.get_ref("refs/heads/test").unwrap(), Some(id));
    }

    #[test]
    fn open_or_init_cache_reopens_existing_store() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");

        // First call initializes and writes a ref.
        {
            let mut store = open_or_init_cache(&cache).unwrap();
            let id = fake_panproto_id(0x55);
            store.set_ref("refs/heads/persistent", id).unwrap();
        }

        // Second call reopens and the ref is still there.
        let store = open_or_init_cache(&cache).unwrap();
        assert_eq!(
            store.get_ref("refs/heads/persistent").unwrap(),
            Some(fake_panproto_id(0x55))
        );
    }

    // ── Integration tests for the push and fetch pipelines ─────────────

    /// Create a git repo with a single file committed `n` times.
    ///
    /// Returns the tempdir (so the repo lives for the test), the repo,
    /// and the commit OIDs in chronological order.
    fn linear_git_history(n: usize) -> (tempfile::TempDir, git2::Repository, Vec<git2::Oid>) {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::new("Tester", "tester@example.com", &git2::Time::new(1000, 0))
            .unwrap();
        let file_path = dir.path().join("main.py");

        let mut commit_oids = Vec::new();
        let mut parent: Option<git2::Oid> = None;

        for i in 0..n {
            std::fs::write(&file_path, format!("x = {i}\n").as_bytes()).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("main.py")).unwrap();
            index.write().unwrap();
            let tree_oid = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let parent_commit = parent.map(|p| repo.find_commit(p).unwrap());
            let parents: Vec<&git2::Commit<'_>> = parent_commit.iter().collect();
            let new_oid = repo
                .commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("commit {i}"),
                    &tree,
                    &parents,
                )
                .unwrap();
            commit_oids.push(new_oid);
            parent = Some(new_oid);
        }

        (dir, repo, commit_oids)
    }

    /// Append one more commit to an existing repo on HEAD and return its OID.
    fn append_commit(repo: &git2::Repository, dir: &Path, n: usize) -> git2::Oid {
        let sig = git2::Signature::new("Tester", "tester@example.com", &git2::Time::new(1000, 0))
            .unwrap();
        let file_path = dir.join("main.py");
        std::fs::write(&file_path, format!("x = {n}\n").as_bytes()).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("main.py")).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("commit {n}"),
            &tree,
            &[&parent],
        )
        .unwrap()
    }

    #[test]
    fn push_stage_first_run_imports_full_history() {
        let (_git_dir, git_repo, oids) = linear_git_history(3);
        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");

        let mut store = open_or_init_cache(&cache).unwrap();
        let report =
            push_import_stage(&mut store, &git_repo, &cache, "HEAD", "refs/heads/main").unwrap();

        // All three commits should be imported on the first run.
        assert_eq!(report.new_commits, 3);
        assert_eq!(report.total_commits, 3);
        assert_ne!(report.head_id, ObjectId::ZERO);

        // Local ref should have been written under the dst name.
        assert_eq!(
            store.get_ref("refs/heads/main").unwrap(),
            Some(report.head_id)
        );

        // Marks file should contain one entry per git commit.
        let marks = load_marks(&marks_path(&cache));
        assert_eq!(marks.len(), 3);
        for oid in &oids {
            assert!(marks.contains_key(oid), "marks missing git OID {oid}");
        }
    }

    #[test]
    fn push_stage_second_run_is_noop_when_nothing_new() {
        let (_git_dir, git_repo, _oids) = linear_git_history(2);
        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");

        let mut store = open_or_init_cache(&cache).unwrap();
        let first =
            push_import_stage(&mut store, &git_repo, &cache, "HEAD", "refs/heads/main").unwrap();
        assert_eq!(first.new_commits, 2);

        // Second run against unchanged history imports nothing.
        let second =
            push_import_stage(&mut store, &git_repo, &cache, "HEAD", "refs/heads/main").unwrap();
        assert_eq!(second.new_commits, 0);
        assert_eq!(second.total_commits, 2, "total should still reflect both");
        assert_eq!(
            second.head_id, first.head_id,
            "head should be preserved across noop push"
        );
    }

    #[test]
    fn push_stage_imports_only_new_commits_after_extension() {
        let (git_dir, git_repo, _oids) = linear_git_history(2);
        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");

        // First push: two commits imported.
        let mut store = open_or_init_cache(&cache).unwrap();
        let first =
            push_import_stage(&mut store, &git_repo, &cache, "HEAD", "refs/heads/main").unwrap();
        assert_eq!(first.new_commits, 2);
        drop(store);

        // Extend the git repo with one more commit.
        append_commit(&git_repo, git_dir.path(), 99);

        // Second push: only the new commit should be imported.
        let mut store = open_or_init_cache(&cache).unwrap();
        let second =
            push_import_stage(&mut store, &git_repo, &cache, "HEAD", "refs/heads/main").unwrap();
        assert_eq!(second.new_commits, 1, "expected just the new commit");
        assert_eq!(second.total_commits, 3);

        // The new head must actually differ from the first head.
        assert_ne!(second.head_id, first.head_id);

        // Marks file should now contain 3 entries.
        let marks = load_marks(&marks_path(&cache));
        assert_eq!(marks.len(), 3);

        // Local ref should have advanced to the new head.
        assert_eq!(
            store.get_ref("refs/heads/main").unwrap(),
            Some(second.head_id)
        );
    }

    #[test]
    fn push_stage_uses_dst_ref_name_not_hardcoded_main() {
        // Regression: the old import path wrote refs/heads/main
        // unconditionally. The new stage writes whatever `dst` names.
        let (_git_dir, git_repo, _oids) = linear_git_history(1);
        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");

        let mut store = open_or_init_cache(&cache).unwrap();
        let report =
            push_import_stage(&mut store, &git_repo, &cache, "HEAD", "refs/heads/feature").unwrap();

        assert_eq!(
            store.get_ref("refs/heads/feature").unwrap(),
            Some(report.head_id)
        );
        assert_eq!(
            store.get_ref("refs/heads/main").unwrap(),
            None,
            "main should not have been written when dst is feature"
        );
    }

    #[test]
    fn fetch_stage_full_export_on_first_run() {
        // Build a git repo, import to a store (simulating what a pull
        // would have populated), then drive the fetch export stage
        // against a fresh destination git repo.
        let (_src_dir, src_repo, _oids) = linear_git_history(3);
        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");

        let mut store = open_or_init_cache(&cache).unwrap();
        // Use the push_import_stage to populate the store and local ref.
        push_import_stage(&mut store, &src_repo, &cache, "HEAD", "refs/heads/main").unwrap();

        // Erase the marks file so fetch_export_stage starts from a clean
        // slate (mimicking a fresh clone: store has objects, marks are
        // empty because we haven't exported anything to git yet).
        std::fs::remove_file(marks_path(&cache)).unwrap();

        let dst_tmp = tempfile::tempdir().unwrap();
        let dst_repo = git2::Repository::init(dst_tmp.path()).unwrap();

        let report = fetch_export_stage(&store, &dst_repo, &cache, "refs/heads/main").unwrap();
        assert_eq!(report.commits_walked, 3);
        assert_eq!(report.commits_exported, 3);
        assert!(report.tip_git_oid.is_some());

        // Verify the exported DAG has correct parent structure (root → 2 parents).
        let tip = dst_repo.find_commit(report.tip_git_oid.unwrap()).unwrap();
        assert_eq!(tip.parent_count(), 1);
        let middle = tip.parent(0).unwrap();
        assert_eq!(middle.parent_count(), 1);
        let root = middle.parent(0).unwrap();
        assert_eq!(root.parent_count(), 0);

        // Marks should record all three exported commits.
        let marks = load_marks(&marks_path(&cache));
        assert_eq!(marks.len(), 3);
    }

    #[test]
    fn fetch_stage_is_idempotent() {
        let (_src_dir, src_repo, _oids) = linear_git_history(2);
        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");

        let mut store = open_or_init_cache(&cache).unwrap();
        push_import_stage(&mut store, &src_repo, &cache, "HEAD", "refs/heads/main").unwrap();
        std::fs::remove_file(marks_path(&cache)).unwrap();

        let dst_tmp = tempfile::tempdir().unwrap();
        let dst_repo = git2::Repository::init(dst_tmp.path()).unwrap();

        let first = fetch_export_stage(&store, &dst_repo, &cache, "refs/heads/main").unwrap();
        assert_eq!(first.commits_exported, 2);

        // Second call against the same destination: nothing new to export.
        let second = fetch_export_stage(&store, &dst_repo, &cache, "refs/heads/main").unwrap();
        assert_eq!(second.commits_exported, 0);
        assert_eq!(second.commits_walked, 2, "walk still covers full DAG");
        assert_eq!(
            second.tip_git_oid, first.tip_git_oid,
            "tip git OID should be reported from marks on noop"
        );
    }

    #[test]
    fn fetch_stage_exports_only_new_commits_after_extension() {
        // Simulate a "fresh clone then fetch again after remote grows":
        // populate the store directly via `import_git_repo_incremental`
        // (which does NOT touch the marks file), then run fetch_export.
        // This models a client that has only ever pulled panproto
        // objects from a remote: no local git history, empty marks.
        //
        // Using `push_import_stage` here would be wrong because push
        // writes marks keyed on the source-repo git OIDs, and in
        // production push and fetch share a single local git repo (so
        // the same OIDs appear on both sides). The generator repo in
        // this test is a different git repo from the destination, so
        // we must not let push marks contaminate fetch state.
        let (src_dir, src_repo, _oids) = linear_git_history(2);
        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");

        let mut store = open_or_init_cache(&cache).unwrap();
        let empty: rustc_hash::FxHashMap<git2::Oid, ObjectId> = rustc_hash::FxHashMap::default();
        let import1 =
            panproto_git::import_git_repo_incremental(&src_repo, &mut store, "HEAD", &empty)
                .unwrap();
        store.set_ref("refs/heads/main", import1.head_id).unwrap();

        let dst_tmp = tempfile::tempdir().unwrap();
        let dst_repo = git2::Repository::init(dst_tmp.path()).unwrap();

        let first = fetch_export_stage(&store, &dst_repo, &cache, "refs/heads/main").unwrap();
        assert_eq!(first.commits_exported, 2);

        // Extend the source and re-import (still without marks writes).
        append_commit(&src_repo, src_dir.path(), 42);
        let import2 =
            panproto_git::import_git_repo_incremental(&src_repo, &mut store, "HEAD", &empty)
                .unwrap();
        store.set_ref("refs/heads/main", import2.head_id).unwrap();

        // Fetch export should only touch the new panproto commit; the
        // previously-exported two commits are recorded in the marks
        // file from the first fetch run.
        let second = fetch_export_stage(&store, &dst_repo, &cache, "refs/heads/main").unwrap();
        assert_eq!(second.commits_exported, 1);
        assert_eq!(second.commits_walked, 3);

        // The new tip must be a direct descendant of the first tip in
        // the destination git repo.
        let new_tip = dst_repo.find_commit(second.tip_git_oid.unwrap()).unwrap();
        assert_eq!(new_tip.parent_count(), 1);
        assert_eq!(new_tip.parent(0).unwrap().id(), first.tip_git_oid.unwrap());
    }

    #[test]
    fn fetch_stage_treats_push_marks_as_already_exported() {
        // Document the production-equivalent behavior: when marks come
        // from a push (i.e. the git OIDs live in the same local repo we
        // are fetching into), fetch_export_stage correctly skips those
        // commits instead of re-exporting them.
        //
        // In production there is only one local git repo and `src_repo`
        // here plays both roles: we push from it and fetch into it.
        let (_git_dir, repo, _oids) = linear_git_history(2);
        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");

        let mut store = open_or_init_cache(&cache).unwrap();
        push_import_stage(&mut store, &repo, &cache, "HEAD", "refs/heads/main").unwrap();

        // Now fetch into the same repo that we pushed from. Marks from
        // the push already cover both commits, so nothing new should
        // be exported.
        let report = fetch_export_stage(&store, &repo, &cache, "refs/heads/main").unwrap();
        assert_eq!(report.commits_exported, 0);
        assert_eq!(report.commits_walked, 2);
        assert!(
            report.tip_git_oid.is_some(),
            "tip should be reported from the push marks even on noop"
        );
    }

    #[test]
    fn fetch_stage_preserves_dag_parent_links_via_marks() {
        // Export commits one at a time, confirming that after each
        // export the marks file lets the next export find its parent.
        let (_src_dir, src_repo, _oids) = linear_git_history(3);
        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");

        let mut store = open_or_init_cache(&cache).unwrap();
        push_import_stage(&mut store, &src_repo, &cache, "HEAD", "refs/heads/main").unwrap();
        std::fs::remove_file(marks_path(&cache)).unwrap();

        let dst_tmp = tempfile::tempdir().unwrap();
        let dst_repo = git2::Repository::init(dst_tmp.path()).unwrap();

        let report = fetch_export_stage(&store, &dst_repo, &cache, "refs/heads/main").unwrap();
        assert_eq!(report.commits_exported, 3);

        // Verify that each non-root commit in the exported git DAG has a
        // parent also present in the marks file (i.e. parent lookup via
        // the marks actually succeeded and wired the git parent).
        let marks = load_marks(&marks_path(&cache));
        let mut git_oids: std::collections::HashSet<git2::Oid> = marks.keys().copied().collect();
        git_oids.insert(report.tip_git_oid.unwrap());

        for git_oid in &git_oids {
            let commit = dst_repo.find_commit(*git_oid).unwrap();
            if commit.parent_count() > 0 {
                let parent_id = commit.parent(0).unwrap().id();
                assert!(
                    git_oids.contains(&parent_id),
                    "exported commit {git_oid} has parent {parent_id} outside the marks set"
                );
            }
        }
    }

    #[test]
    fn fetch_stage_handles_non_monotonic_timestamps() {
        // Regression: git commits can have arbitrary author timestamps
        // (rebases, amends, `--date`). A chronological walk
        // (`log_walk` + reverse) can emit a child before its parent if
        // the child has an earlier timestamp than the parent, causing
        // `export_to_git`'s parent lookup to silently drop the parent
        // and disconnect the exported DAG.
        //
        // This test builds a two-commit panproto history directly in a
        // store, where the child's panproto `timestamp` is SMALLER than
        // the parent's, and verifies that `fetch_export_stage` still
        // produces a connected git DAG (child → parent). A topological
        // walk is required to make this work; the chronological walk
        // would disconnect them.
        use panproto_protocols::raw_file;
        use panproto_schema::SchemaBuilder;
        use panproto_vcs::{CommitObject, Object as VcsObject};

        let proto = raw_file::protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("root", "file", None::<&str>)
            .unwrap()
            .build()
            .unwrap();

        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");
        let mut store = open_or_init_cache(&cache).unwrap();

        // Put the same schema object once and reuse its ID for both
        // commits so we don't have to build two fixtures.
        let schema_id = store.put(&VcsObject::Schema(Box::new(schema))).unwrap();

        // Parent commit: LARGE timestamp.
        let parent_commit =
            CommitObject::builder(schema_id, "project", "Tester", "parent (later timestamp)")
                .timestamp(2000)
                .build();
        let parent_id = store.put(&VcsObject::Commit(parent_commit)).unwrap();

        // Child commit: SMALLER timestamp than the parent. This is the
        // non-monotonic case that would defeat a chronological walk.
        let child_commit =
            CommitObject::builder(schema_id, "project", "Tester", "child (earlier timestamp)")
                .parents(vec![parent_id])
                .timestamp(1000)
                .build();
        let child_id = store.put(&VcsObject::Commit(child_commit)).unwrap();

        store.set_ref("refs/heads/main", child_id).unwrap();

        let dst_tmp = tempfile::tempdir().unwrap();
        let dst_repo = git2::Repository::init(dst_tmp.path()).unwrap();

        let report = fetch_export_stage(&store, &dst_repo, &cache, "refs/heads/main").unwrap();
        assert_eq!(report.commits_walked, 2);
        assert_eq!(report.commits_exported, 2);

        // The exported git DAG must still have the child pointing at
        // the parent, even though the panproto timestamps are inverted.
        let tip_git_oid = report.tip_git_oid.unwrap();
        let tip = dst_repo.find_commit(tip_git_oid).unwrap();
        assert_eq!(
            tip.parent_count(),
            1,
            "child git commit must still have its parent wired, despite non-monotonic timestamps"
        );
        // And the tip message should be the child's, not the parent's.
        assert_eq!(tip.message().unwrap_or(""), "child (earlier timestamp)");
        assert_eq!(
            tip.parent(0).unwrap().message().unwrap_or(""),
            "parent (later timestamp)"
        );
    }

    #[test]
    fn fetch_stage_handles_merge_commit_with_multiple_parents() {
        // A merge commit has multiple parents. The topological walk
        // must visit all ancestors of every parent before emitting the
        // merge itself, and the merge's exported git commit must retain
        // all parent links.
        use panproto_protocols::raw_file;
        use panproto_schema::SchemaBuilder;
        use panproto_vcs::{CommitObject, Object as VcsObject};

        let proto = raw_file::protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("root", "file", None::<&str>)
            .unwrap()
            .build()
            .unwrap();

        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");
        let mut store = open_or_init_cache(&cache).unwrap();

        let schema_id = store.put(&VcsObject::Schema(Box::new(schema))).unwrap();

        // Common root.
        let root_id = store
            .put(&VcsObject::Commit(
                CommitObject::builder(schema_id, "project", "T", "root")
                    .timestamp(1000)
                    .build(),
            ))
            .unwrap();

        // Two parallel branches off the root.
        let left_id = store
            .put(&VcsObject::Commit(
                CommitObject::builder(schema_id, "project", "T", "left")
                    .parents(vec![root_id])
                    .timestamp(2000)
                    .build(),
            ))
            .unwrap();
        let right_id = store
            .put(&VcsObject::Commit(
                CommitObject::builder(schema_id, "project", "T", "right")
                    .parents(vec![root_id])
                    .timestamp(2000)
                    .build(),
            ))
            .unwrap();

        // Merge commit with both branches as parents.
        let merge_id = store
            .put(&VcsObject::Commit(
                CommitObject::builder(schema_id, "project", "T", "merge")
                    .parents(vec![left_id, right_id])
                    .timestamp(3000)
                    .build(),
            ))
            .unwrap();

        store.set_ref("refs/heads/main", merge_id).unwrap();

        let dst_tmp = tempfile::tempdir().unwrap();
        let dst_repo = git2::Repository::init(dst_tmp.path()).unwrap();

        let report = fetch_export_stage(&store, &dst_repo, &cache, "refs/heads/main").unwrap();
        assert_eq!(
            report.commits_walked, 4,
            "should walk all 4 commits (root, left, right, merge)"
        );
        assert_eq!(report.commits_exported, 4);

        // The merge commit in the destination repo must have two parents.
        let tip = dst_repo.find_commit(report.tip_git_oid.unwrap()).unwrap();
        assert_eq!(
            tip.parent_count(),
            2,
            "merge commit should retain both parents in the exported git DAG"
        );
        assert_eq!(tip.message().unwrap_or(""), "merge");
    }

    #[test]
    fn fetch_stage_drops_stale_marks_that_reference_missing_git_commits() {
        // Regression: if the marks file references git OIDs that don't
        // exist in the destination repo (e.g., after a `git gc`, or
        // because a cache was copied between repos), the topological
        // walk must drop those stale entries at load time. Otherwise
        // `export_to_git`'s `git_repo.find_commit(parent)` fails and
        // silently drops the parent, disconnecting the exported DAG.
        //
        // We simulate this by seeding a cache with a marks file whose
        // entries reference entirely fabricated git OIDs, then fetching
        // into a fresh destination repo where those OIDs don't exist.
        // The filter should drop ALL stale marks so the topological
        // walk re-exports every commit into a fully connected DAG.
        let (_src_dir, src_repo, _) = e2e_git_history(2);
        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");

        // Populate the store with panproto objects (simulating a pull).
        let mut store = open_or_init_cache(&cache).unwrap();
        let empty: rustc_hash::FxHashMap<git2::Oid, ObjectId> = rustc_hash::FxHashMap::default();
        let import_result =
            panproto_git::import_git_repo_incremental(&src_repo, &mut store, "HEAD", &empty)
                .unwrap();
        store
            .set_ref("refs/heads/main", import_result.head_id)
            .unwrap();

        // Seed the marks file with fabricated git OIDs — nothing that
        // could exist in the destination repo. These simulate a cache
        // that survived a `git gc` of the old dst.
        let stale_a = git2::Oid::from_str("0123456789abcdef0123456789abcdef01234567").unwrap();
        let stale_b = git2::Oid::from_str("fedcba9876543210fedcba9876543210fedcba98").unwrap();
        let stale_marks = vec![
            (stale_a, import_result.oid_map[0].1),
            (stale_b, import_result.oid_map[1].1),
        ];
        append_marks(&marks_path(&cache), &stale_marks).unwrap();
        assert_eq!(load_marks(&marks_path(&cache)).len(), 2);

        // Fresh destination repo: none of the marked git OIDs exist here.
        let dst_tmp = tempfile::tempdir().unwrap();
        let dst_repo = git2::Repository::init(dst_tmp.path()).unwrap();

        let report = fetch_export_stage(&store, &dst_repo, &cache, "refs/heads/main").unwrap();

        // All marks pointed at non-existent git OIDs, so the filter
        // should have dropped them all; both commits must be re-exported.
        assert_eq!(
            report.commits_exported, 2,
            "both commits should be re-exported when all marks are stale"
        );
        assert_eq!(report.commits_walked, 2);

        // The re-exported tip must have its parent wired, and both
        // commits must be findable in the dst repo (no silent drops).
        let tip = dst_repo.find_commit(report.tip_git_oid.unwrap()).unwrap();
        assert_eq!(
            tip.parent_count(),
            1,
            "re-exported tip must still have its parent wired"
        );
        let parent = tip.parent(0).unwrap();
        assert_ne!(
            parent.id(),
            stale_a,
            "parent should be the freshly-exported root, not the stale OID"
        );
        assert_ne!(parent.id(), stale_b);

        // After the re-export, the marks file carries the fresh git
        // OIDs alongside the old stale ones (the marks file is
        // append-only; old lines are not rewritten). Both panproto IDs
        // should now have at least one VALID entry — a git OID that
        // actually resolves in the destination. That's what the filter
        // in `fetch_export_stage` consults, so the next fetch will see
        // the fresh mapping and ignore the stale one.
        let final_marks = load_marks(&marks_path(&cache));
        let valid_panproto_ids: std::collections::HashSet<ObjectId> = final_marks
            .iter()
            .filter_map(|(g, p)| {
                if dst_repo.find_commit(*g).is_ok() {
                    Some(*p)
                } else {
                    None
                }
            })
            .collect();
        assert!(
            valid_panproto_ids.contains(&import_result.oid_map[0].1),
            "root commit should have a valid git mapping after re-export"
        );
        assert!(
            valid_panproto_ids.contains(&import_result.oid_map[1].1),
            "tip commit should have a valid git mapping after re-export"
        );
    }

    // ── RemoteClient fake + cmd_push/cmd_fetch end-to-end tests ───────

    /// A single call recorded by `FakeRemoteClient` for later inspection.
    #[derive(Clone, Debug)]
    enum RemoteCall {
        Pull,
        Push {
            /// Panproto object IDs present in the store at push time.
            object_ids: std::collections::BTreeSet<ObjectId>,
            /// Refs in the store at push time, sorted by name.
            refs: Vec<(String, ObjectId)>,
        },
        GetRef {
            ref_name: String,
        },
        SetRef {
            ref_name: String,
            old_target: Option<ObjectId>,
            new_target: ObjectId,
            protocol: String,
            commit_count: u64,
        },
    }

    /// In-process fake for `RemoteClient`. Owns a `MemStore` representing
    /// the "server's" view of the repo so that pull/push behave like a
    /// real round-trip: pushed objects and refs appear on the server, and
    /// subsequent pulls see them.
    ///
    /// All network-visible state is tracked in `RefCell`s so the fake's
    /// methods can stay `&self`, matching the real trait.
    struct FakeRemoteClient {
        server: RefCell<MemStore>,
        calls: RefCell<Vec<RemoteCall>>,
    }

    impl FakeRemoteClient {
        fn new() -> Self {
            Self {
                server: RefCell::new(MemStore::new()),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<RemoteCall> {
            self.calls.borrow().clone()
        }

        /// Pre-populate the fake server with objects and refs so a
        /// subsequent `cmd_fetch` has something to pull.
        fn seed_from(&self, store: &FsStore) {
            let mut server = self.server.borrow_mut();
            for id in store.list_objects().unwrap() {
                let obj = store.get(&id).unwrap();
                server.put(&obj).unwrap();
            }
            for (name, id) in store.list_refs("refs/").unwrap() {
                server.set_ref(&name, id).unwrap();
            }
        }
    }

    impl RemoteClient for FakeRemoteClient {
        async fn remote_pull(&self, store: &mut FsStore) -> Result<(), Box<dyn std::error::Error>> {
            self.calls.borrow_mut().push(RemoteCall::Pull);

            // Mimic `NodeClient::pull`: copy all server objects into the
            // local store, then copy all server refs.
            let server = self.server.borrow();
            for id in server.list_objects()? {
                if !store.has(&id) {
                    let obj = server.get(&id)?;
                    store.put(&obj)?;
                }
            }
            for (name, id) in server.list_refs("refs/")? {
                store.set_ref(&name, id)?;
            }
            Ok(())
        }

        async fn remote_push(&self, store: &FsStore) -> Result<(), Box<dyn std::error::Error>> {
            // Snapshot the local state for the test to inspect later.
            let object_ids: std::collections::BTreeSet<ObjectId> =
                store.list_objects()?.into_iter().collect();
            let mut refs = store.list_refs("refs/")?;
            refs.sort_by(|a, b| a.0.cmp(&b.0));
            self.calls.borrow_mut().push(RemoteCall::Push {
                object_ids: object_ids.clone(),
                refs: refs.clone(),
            });

            // Actually mirror the objects and refs onto the server.
            let mut server = self.server.borrow_mut();
            for id in &object_ids {
                if !server.has(id) {
                    let obj: Object = store.get(id)?;
                    server.put(&obj)?;
                }
            }
            for (name, id) in refs {
                server.set_ref(&name, id)?;
            }
            Ok(())
        }

        async fn remote_get_ref(
            &self,
            ref_name: &str,
        ) -> Result<Option<ObjectId>, Box<dyn std::error::Error>> {
            self.calls.borrow_mut().push(RemoteCall::GetRef {
                ref_name: ref_name.to_owned(),
            });
            Ok(self.server.borrow().get_ref(ref_name)?)
        }

        async fn remote_set_ref(
            &self,
            ref_name: &str,
            old_target: Option<&ObjectId>,
            new_target: &ObjectId,
            protocol: &str,
            commit_count: u64,
        ) -> Result<(), Box<dyn std::error::Error>> {
            self.calls.borrow_mut().push(RemoteCall::SetRef {
                ref_name: ref_name.to_owned(),
                old_target: old_target.copied(),
                new_target: *new_target,
                protocol: protocol.to_owned(),
                commit_count,
            });
            self.server.borrow_mut().set_ref(ref_name, *new_target)?;
            Ok(())
        }
    }

    /// Create a git repo with `n` sequential single-file commits. Each
    /// commit gets a distinct monotonic timestamp so tests that pick
    /// the "tip" by committer time work deterministically.
    fn e2e_git_history(n: usize) -> (tempfile::TempDir, git2::Repository, Vec<git2::Oid>) {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let file_path = dir.path().join("main.py");

        let mut commit_oids = Vec::new();
        let mut parent: Option<git2::Oid> = None;

        for i in 0..n {
            let sig = git2::Signature::new(
                "Tester",
                "tester@example.com",
                &git2::Time::new(1000 + i64::try_from(i).unwrap(), 0),
            )
            .unwrap();
            std::fs::write(&file_path, format!("x = {i}\n").as_bytes()).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("main.py")).unwrap();
            index.write().unwrap();
            let tree_oid = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let parent_commit = parent.map(|p| repo.find_commit(p).unwrap());
            let parents: Vec<&git2::Commit<'_>> = parent_commit.iter().collect();
            let new_oid = repo
                .commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("commit {i}"),
                    &tree,
                    &parents,
                )
                .unwrap();
            commit_oids.push(new_oid);
            parent = Some(new_oid);
        }

        (dir, repo, commit_oids)
    }

    /// Run an async body on a current-thread runtime for testing.
    fn run_async<F, T>(fut: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(fut)
    }

    #[test]
    fn cmd_push_end_to_end_calls_remote_in_expected_order() {
        let (_git_dir, git_repo, _oids) = e2e_git_history(2);
        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");

        let fake = FakeRemoteClient::new();
        run_async(cmd_push(&fake, "HEAD:refs/heads/main", &git_repo, &cache)).unwrap();

        // Expected call order: Push, GetRef, SetRef. No Pull.
        //
        // cmd_push's trailing SetRef runs AFTER `remote_push` has
        // already mirrored the local refs to the server, so it is
        // effectively a metadata-correction call (it pins the canonical
        // `commit_count` for the dst ref). The test pins that observed
        // behavior.
        let calls = fake.calls();
        assert_eq!(calls.len(), 3);
        assert!(matches!(calls[0], RemoteCall::Push { .. }));
        match &calls[1] {
            RemoteCall::GetRef { ref_name } => assert_eq!(ref_name, "refs/heads/main"),
            other => panic!("expected GetRef, got {other:?}"),
        }
        match &calls[2] {
            RemoteCall::SetRef {
                ref_name,
                new_target,
                old_target,
                protocol,
                commit_count,
            } => {
                assert_eq!(ref_name, "refs/heads/main");
                assert_eq!(protocol, "project");
                assert_eq!(*commit_count, 2, "two commits total on first push");
                assert_ne!(*new_target, ObjectId::ZERO);
                // The prior GetRef saw what remote_push wrote (the new
                // head) and cmd_push passes that as the CAS old_target.
                assert_eq!(
                    *old_target,
                    Some(*new_target),
                    "trailing SetRef CAS should match what remote_push wrote"
                );
            }
            other => panic!("expected SetRef, got {other:?}"),
        }
    }

    #[test]
    fn cmd_push_end_to_end_push_payload_contains_local_ref() {
        let (_git_dir, git_repo, _oids) = e2e_git_history(1);
        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");

        let fake = FakeRemoteClient::new();
        run_async(cmd_push(
            &fake,
            "HEAD:refs/heads/feature",
            &git_repo,
            &cache,
        ))
        .unwrap();

        // The first call should be Push; its snapshot should contain the
        // feature branch ref (not main) and at least one object.
        let calls = fake.calls();
        match &calls[0] {
            RemoteCall::Push { object_ids, refs } => {
                assert!(!object_ids.is_empty(), "push should have some objects");
                assert!(
                    refs.iter().any(|(name, _)| name == "refs/heads/feature"),
                    "local ref should be refs/heads/feature, got {refs:?}"
                );
                assert!(
                    !refs.iter().any(|(name, _)| name == "refs/heads/main"),
                    "main should NOT be set when dst is feature"
                );
            }
            other => panic!("expected Push first, got {other:?}"),
        }
    }

    #[test]
    fn cmd_push_end_to_end_second_call_grows_commit_count_and_head() {
        // Verify that a second cmd_push call after the git history
        // has been extended:
        // (a) advances the remote head to the new tip, and
        // (b) reports the total commit count, not just the new ones.
        let (git_dir, git_repo, _oids) = e2e_git_history(1);
        let cache_tmp = tempfile::tempdir().unwrap();
        let cache = cache_tmp.path().join("cache");

        let fake = FakeRemoteClient::new();

        // First push: single commit.
        run_async(cmd_push(&fake, "HEAD:refs/heads/main", &git_repo, &cache)).unwrap();
        let first_new_target = match fake.calls().last() {
            Some(RemoteCall::SetRef {
                new_target,
                commit_count,
                ..
            }) => {
                assert_eq!(*commit_count, 1, "first push: one commit total");
                *new_target
            }
            _ => panic!("expected trailing SetRef from first push"),
        };

        // Extend git, push again.
        append_commit(&git_repo, git_dir.path(), 2);
        run_async(cmd_push(&fake, "HEAD:refs/heads/main", &git_repo, &cache)).unwrap();

        // The final call should be SetRef advancing the head, with
        // commit_count = 2 (total history depth).
        match fake.calls().last() {
            Some(RemoteCall::SetRef {
                old_target,
                new_target,
                commit_count,
                ..
            }) => {
                assert_ne!(
                    *new_target, first_new_target,
                    "head should advance to the new tip"
                );
                assert_eq!(*commit_count, 2, "total commit count should now be 2");
                // After remote_push has mirrored the new head, the
                // trailing GetRef reads the new head, so CAS old_target
                // matches new_target (see comment in cmd_push).
                assert_eq!(*old_target, Some(*new_target));
            }
            other => panic!("expected trailing SetRef, got {other:?}"),
        }
    }

    #[test]
    fn cmd_fetch_end_to_end_pulls_and_exports_history() {
        // First, build a "server state" by running cmd_push against a
        // source repo + fake client. Then instantiate a NEW fake that
        // inherits that server state via `seed_from` and a NEW local
        // cache, and run cmd_fetch against a fresh destination repo.
        let (_src_git, src_repo, _) = e2e_git_history(3);
        let push_cache_tmp = tempfile::tempdir().unwrap();
        let push_cache = push_cache_tmp.path().join("cache");
        let push_fake = FakeRemoteClient::new();
        run_async(cmd_push(
            &push_fake,
            "HEAD:refs/heads/main",
            &src_repo,
            &push_cache,
        ))
        .unwrap();

        // Simulate a fresh client fetching from the same server. Build a
        // new fake whose server carries the pushed state.
        let fetch_fake = FakeRemoteClient::new();
        let pushed_server_store = open_or_init_cache(&push_cache).unwrap();
        fetch_fake.seed_from(&pushed_server_store);

        // Fetch into a fresh destination git repo with a fresh cache.
        let fetch_cache_tmp = tempfile::tempdir().unwrap();
        let fetch_cache = fetch_cache_tmp.path().join("cache");
        let dst_tmp = tempfile::tempdir().unwrap();
        let dst_repo = git2::Repository::init(dst_tmp.path()).unwrap();

        run_async(cmd_fetch(
            &fetch_fake,
            "refs/heads/main",
            &dst_repo,
            &fetch_cache,
        ))
        .unwrap();

        // Fetch should have made exactly one Pull call.
        let calls = fetch_fake.calls();
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0], RemoteCall::Pull));

        // The destination repo should now contain three git commits in
        // a chain. Walk from the marks tip.
        let marks = load_marks(&marks_path(&fetch_cache));
        assert_eq!(marks.len(), 3, "all three commits should have marks");
        for oid in marks.keys() {
            // Each recorded git OID should resolve to an actual git commit.
            assert!(
                dst_repo.find_commit(*oid).is_ok(),
                "marks entry {oid} missing from dst repo"
            );
        }
    }

    #[test]
    fn cmd_fetch_end_to_end_second_fetch_is_noop() {
        // Set up server state via an initial push.
        let (_src_git, src_repo, _) = e2e_git_history(2);
        let push_cache_tmp = tempfile::tempdir().unwrap();
        let push_cache = push_cache_tmp.path().join("cache");
        let push_fake = FakeRemoteClient::new();
        run_async(cmd_push(
            &push_fake,
            "HEAD:refs/heads/main",
            &src_repo,
            &push_cache,
        ))
        .unwrap();

        // First fetch onto a fresh client.
        let fetch_fake = FakeRemoteClient::new();
        fetch_fake.seed_from(&open_or_init_cache(&push_cache).unwrap());
        let fetch_cache_tmp = tempfile::tempdir().unwrap();
        let fetch_cache = fetch_cache_tmp.path().join("cache");
        let dst_tmp = tempfile::tempdir().unwrap();
        let dst_repo = git2::Repository::init(dst_tmp.path()).unwrap();

        run_async(cmd_fetch(
            &fetch_fake,
            "refs/heads/main",
            &dst_repo,
            &fetch_cache,
        ))
        .unwrap();
        let marks_after_first = load_marks(&marks_path(&fetch_cache));
        assert_eq!(marks_after_first.len(), 2);

        // Second fetch against the same (unchanged) server should not
        // grow the marks file.
        run_async(cmd_fetch(
            &fetch_fake,
            "refs/heads/main",
            &dst_repo,
            &fetch_cache,
        ))
        .unwrap();
        let marks_after_second = load_marks(&marks_path(&fetch_cache));
        assert_eq!(
            marks_after_second.len(),
            2,
            "second fetch should not add any marks"
        );

        // But we DID call pull both times.
        let pulls = fetch_fake
            .calls()
            .iter()
            .filter(|c| matches!(c, RemoteCall::Pull))
            .count();
        assert_eq!(pulls, 2);
    }

    #[test]
    fn cmd_push_then_fetch_round_trip_preserves_commit_count() {
        // End-to-end round trip: push a repo to a fake server via
        // cmd_push, then pull the same state back via cmd_fetch against
        // a fresh destination. The destination git repo should end up
        // with the same number of commits as the source.
        let (_src_git, src_repo, src_oids) = e2e_git_history(3);
        let push_cache_tmp = tempfile::tempdir().unwrap();
        let push_cache = push_cache_tmp.path().join("cache");
        let fake = FakeRemoteClient::new();

        run_async(cmd_push(
            &fake,
            "HEAD:refs/heads/main",
            &src_repo,
            &push_cache,
        ))
        .unwrap();

        // Fresh client, same fake server (sharing fake state).
        let fetch_cache_tmp = tempfile::tempdir().unwrap();
        let fetch_cache = fetch_cache_tmp.path().join("cache");
        let dst_tmp = tempfile::tempdir().unwrap();
        let dst_repo = git2::Repository::init(dst_tmp.path()).unwrap();

        run_async(cmd_fetch(&fake, "refs/heads/main", &dst_repo, &fetch_cache)).unwrap();

        // Destination should have 3 git commits (one per source commit).
        let marks = load_marks(&marks_path(&fetch_cache));
        assert_eq!(marks.len(), src_oids.len());

        // Every exported commit should resolve in the destination repo
        // and the DAG should form a chain of length 3. Start from the
        // most recent marked commit (marks.keys() are git OIDs).
        let mut chain_len = 0usize;
        let mut current: Option<git2::Commit<'_>> = marks
            .keys()
            .map(|oid| dst_repo.find_commit(*oid).unwrap())
            .max_by_key(|c| c.time().seconds());
        while let Some(c) = current {
            chain_len += 1;
            current = if c.parent_count() > 0 {
                Some(c.parent(0).unwrap())
            } else {
                None
            };
        }
        assert_eq!(chain_len, 3, "dst repo should have a 3-commit chain");
    }
}
