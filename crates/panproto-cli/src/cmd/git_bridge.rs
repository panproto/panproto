//! CLI commands for the git bridge.

use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use panproto_core::vcs::{MemStore, Store};
use panproto_git::import_git_repo;

/// Import a git repository into panproto-vcs.
pub fn cmd_git_import(repo_path: &Path, revspec: &str, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!("Opening git repository at {}...", repo_path.display());
    }

    let git_repo = git2::Repository::open(repo_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to open git repository at {}", repo_path.display()))?;

    let mut store = MemStore::new();

    if verbose {
        eprintln!("Importing commits from {revspec}...");
    }

    let result = import_git_repo(&git_repo, &mut store, revspec)
        .into_diagnostic()
        .wrap_err("git import failed")?;

    println!(
        "Imported {} commit(s) from {}",
        result.commit_count,
        repo_path.display()
    );
    println!("HEAD: {}", result.head_id);

    if verbose {
        for (git_oid, panproto_id) in &result.oid_map {
            println!("  {git_oid} -> {panproto_id}");
        }
    }

    Ok(())
}

/// Export panproto-vcs history to a git repository.
pub fn cmd_git_export(repo_path: &Path, dest_path: &Path, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!(
            "Exporting from {} to {}...",
            repo_path.display(),
            dest_path.display()
        );
    }

    // Open the panproto repository.
    let panproto_repo = panproto_core::vcs::Repository::open(repo_path)
        .into_diagnostic()
        .wrap_err("failed to open panproto repository")?;

    // Create or open the destination git repo.
    let git_repo = if dest_path.exists() {
        git2::Repository::open(dest_path)
            .into_diagnostic()
            .wrap_err("failed to open destination git repository")?
    } else {
        git2::Repository::init(dest_path)
            .into_diagnostic()
            .wrap_err("failed to initialize destination git repository")?
    };

    // Get HEAD commit ID.
    let head_state = panproto_repo
        .store()
        .get_head()
        .into_diagnostic()
        .wrap_err("failed to read HEAD")?;

    let head_ref = match head_state {
        panproto_core::vcs::HeadState::Branch(name) => panproto_repo
            .store()
            .get_ref(&format!("refs/heads/{name}"))
            .into_diagnostic()
            .wrap_err("failed to resolve HEAD branch")?
            .ok_or_else(|| miette::miette!("HEAD branch {name} has no commits"))?,
        panproto_core::vcs::HeadState::Detached(id) => id,
    };

    let parent_map = rustc_hash::FxHashMap::default();
    let result =
        panproto_git::export_to_git(panproto_repo.store(), &git_repo, head_ref, &parent_map)
            .into_diagnostic()
            .wrap_err("git export failed")?;

    println!(
        "Exported {} file(s) to {}",
        result.file_count,
        dest_path.display()
    );
    println!("Git commit: {}", result.git_oid);

    Ok(())
}
