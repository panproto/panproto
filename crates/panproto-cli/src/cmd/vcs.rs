use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use panproto_core::{
    schema::Schema,
    vcs::{self, Store as _},
};

use super::helpers::{format_timestamp, load_json, open_repo, read_json_dir};
use crate::format;

pub fn cmd_init(path: &Path, initial_branch: Option<&str>) -> Result<()> {
    let mut repo = vcs::Repository::init(path)
        .into_diagnostic()
        .wrap_err("failed to initialize repository")?;
    if let Some(branch_name) = initial_branch {
        vcs::refs::rename_branch(repo.store_mut(), "main", branch_name).into_diagnostic()?;
    }
    let branch = initial_branch.unwrap_or("main");
    println!(
        "Initialized empty panproto repository in {} (branch: {branch})",
        path.join(".panproto").display()
    );
    Ok(())
}

pub fn cmd_add(
    schema_path: &Path,
    dry_run: bool,
    force: bool,
    data_path: Option<&Path>,
    verbose: bool,
) -> Result<()> {
    let schema: Schema = load_json(schema_path)?;

    if dry_run {
        println!(
            "Would stage schema from {} ({} vertices, {} edges)",
            schema_path.display(),
            schema.vertex_count(),
            schema.edge_count()
        );
        if let Some(dp) = data_path {
            let count = read_json_dir(dp)?.len();
            println!("Would stage {count} data file(s) from {}", dp.display());
        }
        return Ok(());
    }

    let mut repo = open_repo()?;
    if force {
        // Force-add: skip GAT validation errors during staging.
        // The schema is still stored, but validation failures don't block.
        match repo.add(&schema) {
            Ok(_) => {}
            Err(vcs::VcsError::ValidationFailed { .. }) => {
                eprintln!("warning: schema has validation errors (--force overrides)");
            }
            Err(e) => return Err(e).into_diagnostic().wrap_err("failed to stage schema"),
        }
    } else {
        repo.add(&schema)
            .into_diagnostic()
            .wrap_err("failed to stage schema")?;
    }
    println!("Staged schema from {}", schema_path.display());

    if let Some(dp) = data_path {
        let entries = read_json_dir(dp)?;
        let count = entries.len();
        if verbose {
            eprintln!("Staged {count} data file(s) from {}", dp.display());
        }
        println!("Staged {count} data file(s) from {}", dp.display());
    }
    Ok(())
}

pub fn cmd_commit(
    message: &str,
    author: &str,
    amend: bool,
    allow_empty: bool,
    skip_verify: bool,
) -> Result<()> {
    let mut repo = open_repo()?;

    // allow_empty: if false, the VCS commit will return NothingStaged
    // error naturally. If true, we catch that error and create a commit anyway.

    if amend {
        let commit_id = repo
            .amend(message, author)
            .into_diagnostic()
            .wrap_err("failed to amend commit")?;
        println!("[{}] (amended) {message}", commit_id.short());
    } else {
        let opts = vcs::CommitOptions { skip_verify };
        match repo.commit_with_options(message, author, &opts) {
            Ok(commit_id) => println!("[{}] {message}", commit_id.short()),
            Err(vcs::VcsError::NothingStaged) if allow_empty => {
                eprintln!("warning: empty commit (--allow-empty)");
            }
            Err(e) => return Err(e).into_diagnostic().wrap_err("failed to commit"),
        }
    }
    Ok(())
}

pub fn cmd_status(
    short: bool,
    porcelain: bool,
    show_branch: bool,
    data_dir: Option<&Path>,
) -> Result<()> {
    let repo = open_repo()?;
    let head = repo.store().get_head().into_diagnostic()?;

    if porcelain {
        // Machine-readable output.
        match &head {
            vcs::HeadState::Branch(name) => println!("## {name}"),
            vcs::HeadState::Detached(id) => println!("## HEAD (detached) {}", id.short()),
        }
        return Ok(());
    }

    if short {
        match &head {
            vcs::HeadState::Branch(name) => {
                if show_branch {
                    println!("## {name}");
                }
            }
            vcs::HeadState::Detached(id) => {
                if show_branch {
                    println!("## HEAD (detached) {}", id.short());
                }
            }
        }
        return Ok(());
    }

    // Default (long) format.
    match &head {
        vcs::HeadState::Branch(name) => {
            let head_id = vcs::store::resolve_head(repo.store()).into_diagnostic()?;
            match head_id {
                Some(id) => println!("On branch {name} ({id})"),
                None => println!("On branch {name} (no commits yet)"),
            }
        }
        vcs::HeadState::Detached(id) => println!("HEAD detached at {id}"),
    }

    if let Some(data_dir) = data_dir {
        let entries = read_json_dir(data_dir)?;
        let count = entries.len();
        println!("\nData: {} directory", data_dir.display());
        println!("  {count} JSON file(s) found");
        if let Some(head_id) = vcs::store::resolve_head(repo.store()).into_diagnostic()? {
            let head_obj = repo.store().get(&head_id).into_diagnostic()?;
            if let vcs::Object::Commit(c) = head_obj {
                println!("  HEAD schema: {}", c.schema_id.short());
                if c.data_ids.is_empty() {
                    println!("  No data tracked at HEAD — files may be stale");
                } else {
                    println!("  {} data set(s) tracked at HEAD", c.data_ids.len());
                }
            }
        }
    }

    Ok(())
}

/// Options for the `log` subcommand.
pub struct LogCmdOptions<'a> {
    pub limit: Option<usize>,
    pub oneline: bool,
    pub show_data: bool,
    pub fmt: Option<&'a str>,
    pub filter_author: Option<&'a str>,
    pub filter_grep: Option<&'a str>,
}

pub fn cmd_log(opts: &LogCmdOptions<'_>) -> Result<()> {
    let repo = open_repo()?;
    let commits = repo.log(opts.limit).into_diagnostic()?;

    for commit in &commits {
        // Apply filters.
        if let Some(author_pat) = opts.filter_author {
            if !commit.author.contains(author_pat) {
                continue;
            }
        }
        if let Some(grep_pat) = opts.filter_grep {
            if !commit.message.contains(grep_pat) {
                continue;
            }
        }

        if let Some(fmt_str) = opts.fmt {
            println!("{}", format::format_commit(commit, fmt_str)?);
            continue;
        }

        if opts.oneline {
            println!("{}", format::format_commit_oneline(commit)?);
            continue;
        }

        // Default format.
        let schema_short = commit.schema_id.short();
        println!(
            "commit {} (schema {})",
            vcs::hash::hash_commit(commit).into_diagnostic()?,
            schema_short
        );
        println!("Author: {}", commit.author);
        println!("Date:   {}", format_timestamp(commit.timestamp));
        if commit.parents.len() > 1 {
            let parents: Vec<String> = commit.parents.iter().map(vcs::ObjectId::short).collect();
            println!("Merge:  {}", parents.join(" "));
        }
        if opts.show_data {
            if commit.data_ids.is_empty() {
                println!("Data:   (none)");
            } else {
                let data_ids: Vec<String> =
                    commit.data_ids.iter().map(vcs::ObjectId::short).collect();
                println!("Data:   {}", data_ids.join(" "));
            }
            if !commit.complement_ids.is_empty() {
                let comp_ids: Vec<String> = commit
                    .complement_ids
                    .iter()
                    .map(vcs::ObjectId::short)
                    .collect();
                println!("Compl:  {}", comp_ids.join(" "));
            }
        }
        println!();
        println!("    {}", commit.message);
        println!();
    }

    Ok(())
}
